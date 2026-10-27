use serde_json::json;
mod common;
use common::{Harness, failure, success};

#[test]
fn priority_list_preserves_json_and_only_fetches_priorities() {
    let h = Harness::new(
        json!({"body":r#"[{"id":3,"name":"中","unknown":123456789012345678901234567890}]"#}),
    );
    let output = h.run(&["priority", "--json", "list", "--no-input"]);
    let value = success(&output);
    assert_eq!(value["items"][0]["name"], "中");
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("123456789012345678901234567890")
    );
    let requests = h.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0]["path"], "/api/v2/priorities");
    assert!(
        requests[0]["headers"]
            .as_object()
            .unwrap()
            .iter()
            .any(|(name, value)| name.eq_ignore_ascii_case("Backlog-API-Key")
                && value == "test-secret")
    );
}

#[test]
fn resolution_list_displays_zero_and_plain_columns() {
    let h = Harness::new(json!({"body":r#"[{"id":0,"name":"対応済み"}]"#}));
    let output = h.run(&["resolution", "list"]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stderr.is_empty());
    assert_eq!(output.stdout, "ID\tNAME\n0\t対応済み\n".as_bytes());
    assert_eq!(
        success(&h.run(&["resolution", "list", "--json"])),
        json!({"items":[{"id":0,"name":"対応済み"}]})
    );
    let requests = h.requests();
    assert_eq!(requests.len(), 2);
    assert!(requests.iter().all(|r| r["path"] == "/api/v2/resolutions"));
}

#[test]
fn issue_type_list_uses_explicit_project_then_cwd_default_without_project_lookup() {
    let h = Harness::new(json!({"body":r##"[{"id":7,"name":"タスク","color":"#fff"}]"##}));
    std::fs::create_dir(h.root.join(".backlog")).unwrap();
    std::fs::write(
        h.root.join(".backlog/settings.json"),
        r#"{"project":"DEFAULT","issue":{"create":{"assignee":"self"}}}"#,
    )
    .unwrap();
    assert_eq!(
        success(&h.run(&["issue-type", "list", "--json"])),
        json!({"items":[{"id":7,"name":"タスク","color":"#fff"}]})
    );
    let output = h.run(&[
        "--profile",
        "default",
        "issue-type",
        "list",
        "-p",
        "42",
        "--no-input",
    ]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(output.stdout, "ID\tNAME\n7\tタスク\n".as_bytes());
    assert!(output.stderr.is_empty());
    success(&h.run(&["issue-type", "list", "--project=a/b%?#", "--json"]));
    let paths: Vec<_> = h
        .requests()
        .iter()
        .map(|r| r["path"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        paths,
        [
            "/api/v2/projects/DEFAULT/issueTypes",
            "/api/v2/projects/42/issueTypes",
            "/api/v2/projects/a%2Fb%25%3F%23/issueTypes"
        ]
    );
}

fn list_args(command: &str, json: bool) -> Vec<&str> {
    let mut args = vec![command, "list"];
    if command == "issue-type" {
        args.extend(["-p", "P"]);
    }
    if json {
        args.push("--json");
    }
    args
}

#[test]
fn candidate_list_flags_are_scoped_and_project_is_required_without_default() {
    let h = Harness::new(json!({"body":"[]"}));
    failure(&h.run(&["issue-type", "list", "--json"]), 2, "input");
    for project in ["", ".", ".."] {
        failure(
            &h.run(&["issue-type", "list", "--project", project, "--json"]),
            2,
            "input",
        );
    }
    failure(
        &h.run(&["issue-type", "list", "-p", "P", "--project=Q", "--json"]),
        2,
        "input",
    );
    for command in ["issue-type", "priority", "resolution"] {
        for flag in [
            "--status",
            "--assignee",
            "--milestone",
            "--limit",
            "--offset",
            "--keyword",
            "--issue-type-id",
            "--priority-id",
            "--resolution-id",
        ] {
            let mut args = list_args(command, true);
            args.extend([flag, "1"]);
            failure(&h.run(&args), 2, "input");
        }
        for global in ["--json", "--no-input"] {
            let mut args = list_args(command, false);
            args.extend([global, global]);
            if global != "--json" {
                args.push("--json");
            }
            failure(&h.run(&args), 2, "input");
        }
        let mut args = list_args(command, true);
        args.extend(["--profile", "default", "--profile=default"]);
        failure(&h.run(&args), 2, "input");
    }
    for command in ["priority", "resolution"] {
        for flag in ["--project", "-p"] {
            failure(&h.run(&[command, "list", flag, "P", "--json"]), 2, "input");
        }
    }
    assert!(h.requests().is_empty());
}

#[test]
fn candidate_list_empty_success_and_invalid_responses_are_distinct() {
    let h = Harness::new(json!({"body":"[]"}));
    for command in ["issue-type", "priority", "resolution"] {
        assert_eq!(
            success(&h.run(&list_args(command, true))),
            json!({"items":[]})
        );
        let output = h.run(&list_args(command, false));
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert!(output.stderr.is_empty());
        assert_eq!(output.stdout, b"ID\tNAME\n");
        for body in [
            "{}",
            "null",
            "not JSON test-secret",
            r#"[{"id":1}]"#,
            r#"[{"id":1,"name":null}]"#,
            r#"[{"id":-1,"name":"N"}]"#,
        ] {
            std::fs::write(
                h.root.join("response.json"),
                json!({"body":body}).to_string(),
            )
            .unwrap();
            let output = h.run(&list_args(command, true));
            let e = failure(&output, 1, "invalid_response");
            assert_eq!(e["error"]["httpStatus"], 200);
            assert!(
                !String::from_utf8(output.stderr)
                    .unwrap()
                    .contains("test-secret")
            );
        }
        std::fs::write(
            h.root.join("response.json"),
            json!({"body":"[]"}).to_string(),
        )
        .unwrap();
    }
    assert_eq!(h.requests().len(), 24);
}

#[test]
fn candidate_lists_apply_safe_display_and_secret_protection_to_original_json() {
    let h = Harness::new(
        json!({"body":r#"[{"id":1,"name":"日本語 é 👩‍💻\n\ttest-secret\u001b]52;c;X\u0007","test-secret":[{"value":"test-secret"}],"unknown":{"$serde_json::private::Number":"123"}}]"#}),
    );
    for command in ["issue-type", "priority", "resolution"] {
        let output = h.run(&list_args(command, false));
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert!(output.stderr.is_empty());
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.starts_with("ID\tNAME\n1\t日本語 é 👩‍💻\n\t[REDACTED]"));
        assert!(!text.contains("test-secret"));
        assert!(
            !text
                .chars()
                .any(|c| c.is_control() && c != '\n' && c != '\t')
        );
        let output = h.run(&list_args(command, true));
        assert!(
            String::from_utf8(output.stdout.clone())
                .unwrap()
                .contains(r#""unknown":{"$serde_json::private::Number":"123"}"#)
        );
        let value = success(&output);
        assert_eq!(value["items"][0]["[REDACTED]"][0]["value"], "[REDACTED]");
    }
    for command in ["issue-type", "priority", "resolution"] {
        std::fs::write(
            h.root.join("response.json"),
            json!({"body":r#"[{"id":1,"name":"N","test-secret":1,"[REDACTED]":2}]"#}).to_string(),
        )
        .unwrap();
        let e = failure(&h.run(&list_args(command, true)), 1, "output");
        assert_eq!(e["error"]["httpStatus"], 200);
        std::fs::write(
            h.root.join("response.json"),
            json!({"status":403,"body":r#"{"errors":[{"message":"test-secret\u001b[31m"}]}"#})
                .to_string(),
        )
        .unwrap();
        let output = h.run(&list_args(command, true));
        let e = failure(&output, 1, "permission");
        assert_eq!(e["error"]["httpStatus"], 403);
        let text = String::from_utf8(output.stderr).unwrap();
        assert!(!text.contains("test-secret"));
        assert!(text.contains("[REDACTED]"));
    }
}

#[test]
fn candidate_list_help_needs_no_configuration_and_global_flags_select_profile() {
    let h = Harness::new(json!({"body":"[]"}));
    std::fs::write(h.root.join("backlog/config.json"), "broken").unwrap();
    for command in ["issue-type", "priority", "resolution"] {
        for args in [
            vec![command, "--help"],
            vec!["--json", command, "list", "--help", "--no-input"],
        ] {
            let output = h.run(&args);
            assert_eq!(output.status.code(), Some(0), "{output:?}");
            assert!(output.stderr.is_empty());
            assert!(!output.stdout.is_empty());
        }
    }
    assert!(h.requests().is_empty());
    h.config(json!({"profiles":{
        "default":{"spaceUrl":"https://unused.invalid","apiKeyEnv":"MISSING_KEY"},
        "work":{"spaceUrl":h.origin,"apiKeyEnv":"WORK_KEY"}
    }}));
    for command in ["issue-type", "priority", "resolution"] {
        let mut args = vec!["--profile", "work", "--no-input"];
        args.extend(list_args(command, true));
        success(
            &h.command(&args)
                .env("WORK_KEY", "work-secret")
                .output()
                .unwrap(),
        );
        let mut args = list_args(command, false);
        args.extend(["--no-input", "--profile=work", "--json"]);
        success(
            &h.command(&args)
                .env("WORK_KEY", "work-secret")
                .output()
                .unwrap(),
        );
    }
    let requests = h.requests();
    assert_eq!(requests.len(), 6);
    assert!(requests.iter().all(|r| {
        r["method"] == "GET"
            && r["headers"]
                .as_object()
                .unwrap()
                .iter()
                .any(|(name, value)| {
                    name.eq_ignore_ascii_case("Backlog-API-Key") && value == "work-secret"
                })
    }));
}
