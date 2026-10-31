use serde_json::{Value, json};
use std::fs;
use std::process::{Command, Stdio};
mod common;
use common::{Harness, failure, success};

#[test]
fn project_list_uses_header_auth_and_preserves_api_json() {
    let h = Harness::new(
        json!({"body":r#"[{"id":1,"projectKey":"日本語","name":"é 👩‍💻\n\t","unknown":123456789012345678901234567890}]"#}),
    );
    let output = h.run(&[
        "--json",
        "project",
        "list",
        "--no-input",
        "--profile",
        "default",
    ]);
    let value = success(&output);
    assert_eq!(value["items"][0]["projectKey"], "日本語");
    assert_eq!(value["items"][0]["name"], "é 👩‍💻\n\t");
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("123456789012345678901234567890")
    );
    let requests = h.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0]["path"], "/api/v2/projects");
    let headers = requests[0]["headers"].as_object().unwrap();
    assert!(headers.iter().any(
        |(name, value)| name.eq_ignore_ascii_case("Backlog-API-Key") && value == "test-secret"
    ));
}

#[test]
fn project_list_preserves_real_number_token_objects_in_success_and_api_errors() {
    let h = Harness::new(json!({"body":"[]"}));
    for (unknown, expected) in [
        (
            r#"{"$serde_json::private::Number":"123"}"#,
            r#""unknown":{"$serde_json::private::Number":"123"}"#,
        ),
        (
            r#"{"$serde_json::private::Number":"123","sibling":true}"#,
            r#""unknown":{"$serde_json::private::Number":"123","sibling":true}"#,
        ),
        (
            r#"[{"$serde_json::private::Number":"123"},123456789012345678901234567890,"text",null,false]"#,
            r#""unknown":[{"$serde_json::private::Number":"123"},123456789012345678901234567890,"text",null,false]"#,
        ),
        (
            r#"{"$serde_json::private::Number":"test-secret"}"#,
            r#""unknown":{"$serde_json::private::Number":"[REDACTED]"}"#,
        ),
    ] {
        for status in [200, 401] {
            let body = if status == 200 {
                format!(r#"[{{"id":1,"projectKey":"P","name":"N","unknown":{unknown}}}]"#)
            } else {
                format!(r#"{{"errors":[{{"code":11,"unknown":{unknown}}}]}}"#)
            };
            fs::write(
                h.root.join("response.json"),
                json!({"status":status,"body":body}).to_string(),
            )
            .unwrap();
            let output = h.run(&["--json", "project", "list"]);
            let text = if status == 200 {
                assert_eq!(output.status.code(), Some(0), "{output:?}");
                assert!(output.stderr.is_empty(), "{output:?}");
                String::from_utf8(output.stdout).unwrap()
            } else {
                assert_eq!(output.status.code(), Some(1), "{output:?}");
                assert!(output.stdout.is_empty(), "{output:?}");
                let text = String::from_utf8(output.stderr).unwrap();
                assert!(text.contains(r#""kind":"auth""#), "{text}");
                assert!(text.contains(r#""httpStatus":401"#), "{text}");
                text
            };
            assert!(text.contains(expected), "{text}");
            assert!(!text.contains("test-secret"), "{text}");
        }
    }
}

#[test]
fn project_list_json_decode_keeps_the_existing_nesting_limit() {
    let h = Harness::new(json!({"body":"[]"}));
    for depth in [125, 126] {
        let unknown = format!("{}null{}", "[".repeat(depth), "]".repeat(depth));
        let body = format!(r#"[{{"id":1,"projectKey":"P","name":"N","unknown":{unknown}}}]"#);
        fs::write(
            h.root.join("response.json"),
            json!({"body":body}).to_string(),
        )
        .unwrap();
        let output = h.run(&["--json", "project", "list"]);
        if depth == 125 {
            assert_eq!(output.status.code(), Some(0), "{output:?}");
            assert!(output.stderr.is_empty());
            assert!(String::from_utf8(output.stdout).unwrap().contains(&unknown));
        } else {
            let error = failure(&output, 1, "invalid_response");
            assert_eq!(error["error"]["httpStatus"], 200);
        }
    }
}

#[test]
fn project_list_validates_duplicate_values_before_last_value_wins() {
    let h = Harness::new(json!({"body":"[]"}));
    let too_deep = format!("{}null{}", "[".repeat(126), "]".repeat(126));
    for (first, last, expected) in [
        (too_deep.as_str(), "1", None),
        (r#""\uD800""#, "1", None),
        (
            r#"{"$serde_json::private::Number":"123"}"#,
            "1",
            Some(r#""unknown":1"#),
        ),
        (
            "1",
            r#"{"$serde_json::private::Number":"123"}"#,
            Some(r#""unknown":{"$serde_json::private::Number":"123"}"#),
        ),
    ] {
        let body = format!(
            r#"[{{"id":1,"projectKey":"P","name":"N","unknown":{first},"unknown":{last}}}]"#
        );
        fs::write(
            h.root.join("response.json"),
            json!({"body":body}).to_string(),
        )
        .unwrap();
        let output = h.run(&["--json", "project", "list"]);
        if let Some(expected) = expected {
            assert_eq!(output.status.code(), Some(0), "{output:?}");
            assert!(output.stderr.is_empty(), "{output:?}");
            let text = String::from_utf8(output.stdout).unwrap();
            assert!(text.contains(expected), "{text}");
        } else {
            let error = failure(&output, 1, "invalid_response");
            assert_eq!(error["error"]["httpStatus"], 200);
        }
    }
}

#[test]
fn configuration_validates_editor_and_only_cwd_settings() {
    let h = Harness::new(json!({"body":"[]"}));
    h.config(json!({"editor":["/usr/bin/true","--wait"], "profiles":{"default":{"spaceUrl":h.origin,"apiKeyEnv":"TEST_KEY"}}}));
    assert_eq!(
        success(&h.run(&["project", "list", "--json"])),
        json!({"items":[]})
    );
    fs::create_dir(h.root.join(".backlog")).unwrap();
    fs::write(
        h.root.join(".backlog/settings.json"),
        r#"{"profile":"work"}"#,
    )
    .unwrap();
    failure(&h.run(&["project", "list", "--json"]), 1, "config");
    assert_eq!(h.requests().len(), 1);
}

#[test]
fn json_argument_errors_are_structured_even_for_missing_subcommands() {
    let h = Harness::new(json!({"body":"[]"}));
    for args in [
        vec!["--json"],
        vec!["project", "--json"],
        vec!["project", "list", "--json", "--bad"],
        vec!["--json", "project", "list", "--profile"],
        vec![
            "--profile",
            "a",
            "--profile",
            "b",
            "--json",
            "project",
            "list",
        ],
    ] {
        failure(&h.run(&args), 2, "input");
    }
    assert!(h.requests().is_empty());
}

#[test]
fn api_failures_are_not_empty_lists_or_retried_and_redirects_are_not_followed() {
    let h = Harness::new(json!({"body":"[]"}));
    let cases = [
        (
            json!({"status":401,"body":r#"{"errors":[{"code":11,"message":"test-secret\u001b[31m"}]}"#}),
            "auth",
            Some(401),
        ),
        (json!({"status":403}), "permission", Some(403)),
        (json!({"status":404}), "not_found", Some(404)),
        (json!({"status":429}), "rate_limit", Some(429)),
        (
            json!({"status":500,"body":"raw test-secret"}),
            "api",
            Some(500),
        ),
        (
            json!({"status":302,"location":format!("{}/redirect-target?apiKey=wrong", h.origin)}),
            "api",
            Some(302),
        ),
        (
            json!({"body":"not JSON test-secret"}),
            "invalid_response",
            Some(200),
        ),
        (json!({"body":"{}"}), "invalid_response", Some(200)),
        (
            json!({"body":r#"[{"id":0,"projectKey":"P","name":"N"}]"#}),
            "invalid_response",
            Some(200),
        ),
        (
            json!({"body":r#"[{"id":1,"projectKey":"P"}]"#}),
            "invalid_response",
            Some(200),
        ),
        (json!({"disconnect":true}), "network", None),
    ];
    for (index, (response, kind, status)) in cases.into_iter().enumerate() {
        fs::write(h.root.join("response.json"), response.to_string()).unwrap();
        let output = h.run(&["project", "list", "--json"]);
        let error = failure(&output, 1, kind);
        assert_eq!(error["error"]["httpStatus"], json!(status));
        assert!(
            !String::from_utf8(output.stderr)
                .unwrap()
                .contains("test-secret")
        );
        assert_eq!(h.requests().len(), index + 1, "No redirect or retry");
    }
}

#[test]
fn interrupt_before_headers_and_during_body_read_preserves_observed_http_status() {
    for (response, ready, status) in [
        (json!({"body":"[]", "header_delay":32}), "requests", None),
        (json!({"body":"[]", "delay":32}), "headers-sent", Some(200)),
        (
            json!({"status":403, "body":"{}", "delay":32}),
            "headers-sent",
            Some(403),
        ),
    ] {
        let h = Harness::new(response);
        let child = h
            .command(&["--json", "project", "list"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let start = std::time::Instant::now();
        while !h.root.join(ready).exists() {
            assert!(start.elapsed() < std::time::Duration::from_secs(10));
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        if status.is_some() {
            // Allow the client to consume flushed headers before interrupting the stalled body.
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        assert!(
            Command::new("/bin/kill")
                .args(["-INT", &child.id().to_string()])
                .status()
                .unwrap()
                .success()
        );
        let error = failure(&child.wait_with_output().unwrap(), 130, "interrupted");
        assert_eq!(error["error"]["httpStatus"], json!(status));
        assert_eq!(h.requests().len(), 1);
    }
}

#[test]
fn project_list_has_fixed_plain_columns_and_safe_json_with_collision_errors() {
    let h = Harness::new(
        json!({"body":r#"[{"id":1,"projectKey":"P","name":"日本語 é 👩‍💻\n\ttest-secret\u001b]52;c;X\u0007\u009b31m","test-secret":{"a":["test-secret"]}}]"#}),
    );
    let output = h.run(&["project", "list"]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stderr.is_empty());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.starts_with("ID\tKEY\tNAME\n1\tP\t日本語 é 👩‍💻\n\t[REDACTED]"));
    assert!(!text.contains("test-secret"));
    assert!(
        !text
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\t')
    );
    let value = success(&h.run(&["project", "--json", "list"]));
    assert_eq!(value["items"][0]["[REDACTED]"]["a"][0], "[REDACTED]");
    fs::write(
        h.root.join("response.json"),
        json!({"body":r#"[{"id":1,"projectKey":"P","name":"N","test-secret":1,"[REDACTED]":2}]"#})
            .to_string(),
    )
    .unwrap();
    failure(&h.run(&["project", "list", "--json"]), 1, "output");
    failure(
        &h.run(&["--json", "project", "list", "--bad=test-secret"]),
        2,
        "input",
    );
}

#[test]
fn configuration_rejects_unknown_keys_types_invalid_origins_and_editor_argv_before_http() {
    use std::os::unix::fs::PermissionsExt;
    let h = Harness::new(json!({"body":"[]"}));
    let valid = json!({"profiles":{"default":{"spaceUrl":h.origin,"apiKeyEnv":"TEST_KEY"}}});
    let mut invalid = vec![
        json!({}),
        json!({"profiles":null}),
        json!({"profiles":[]}),
        json!({"profiles":{},"unknown":true}),
        json!({"profiles":{"default":{"spaceUrl":h.origin,"apiKeyEnv":"TEST_KEY","apiKey":"forbidden"}}}),
        json!({"profiles":{"default":{"spaceUrl":1,"apiKeyEnv":"TEST_KEY"}}}),
        json!({"profiles":{"default":{"spaceUrl":h.origin}}}),
    ];
    for editor in [
        json!([]),
        json!([""]),
        json!(["/does/not/exist"]),
        json!(["/tmp"]),
        json!(["/usr/bin/true", "\0"]),
        json!("vim"),
        json!(null),
        json!([1]),
    ] {
        let mut config = valid.clone();
        config["editor"] = editor;
        invalid.push(config);
    }
    let no_exec = h.root.join("non-executable");
    fs::write(&no_exec, "not executable").unwrap();
    fs::set_permissions(&no_exec, fs::Permissions::from_mode(0o600)).unwrap();
    let mut config = valid.clone();
    config["editor"] = json!([no_exec]);
    invalid.push(config);
    for url in [
        "http://example.com",
        "https://user@example.com",
        "https://@example.com",
        "https://example.com/a",
        "https://example.com/../",
        "https://example.com?x=1",
        "https://example.com#x",
        "https://example.com\\evil",
        "https://",
        "https://exa\nmple.com",
    ] {
        let mut config = valid.clone();
        config["profiles"]["default"]["spaceUrl"] = json!(url);
        invalid.push(config);
    }
    for variable in ["", "   ", "X=Y", "X\0Y"] {
        let mut config = valid.clone();
        config["profiles"]["default"]["apiKeyEnv"] = json!(variable);
        invalid.push(config);
    }
    for config in invalid {
        h.config(config);
        failure(&h.run(&["--json", "project", "list"]), 1, "config");
    }
    assert!(h.requests().is_empty());
}

#[test]
fn cwd_settings_are_strict_at_every_level_but_unused_defaults_do_not_trigger_apis() {
    let h = Harness::new(json!({"body":"[]"}));
    fs::create_dir(h.root.join(".backlog")).unwrap();
    let path = h.root.join(".backlog/settings.json");
    for settings in [
        json!({"project":" MYAPP ","issue":{"create":{"issueType":"Task","priority":"Medium","assignee":"self"},"list":{"status":["Open","In Progress"]}}}),
        json!({"issue":{"list":{"status":[]}}}),
        json!({}),
    ] {
        fs::write(&path, settings.to_string()).unwrap();
        assert_eq!(
            success(&h.run(&["--json", "project", "list"])),
            json!({"items":[]})
        );
    }
    for settings in [
        json!({"unknown":1}),
        json!({"profile":"work"}),
        json!({"project":1}),
        json!({"project":null}),
        json!({"project":""}),
        json!({"project":" \t"}),
        json!({"issue":null}),
        json!({"issue":{"unknown":1}}),
        json!({"issue":{"create":null}}),
        json!({"issue":{"create":{"unknown":1}}}),
        json!({"issue":{"create":{"issueType":1}}}),
        json!({"issue":{"create":{"priority":" "}}}),
        json!({"issue":{"create":{"assignee":null}}}),
        json!({"issue":{"list":{"status":null}}}),
        json!({"issue":{"list":{"status":"Open"}}}),
        json!({"issue":{"list":{"status":[""]}}}),
        json!({"issue":{"list":{"status":[1]}}}),
        json!({"issue":{"list":{"unknown":1}}}),
    ] {
        fs::write(&path, settings.to_string()).unwrap();
        failure(&h.run(&["--json", "project", "list"]), 1, "config");
    }
    fs::write(&path, "{broken").unwrap();
    failure(&h.run(&["--json", "project", "list"]), 1, "config");
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    failure(&h.run(&["--json", "project", "list"]), 1, "config");
    assert_eq!(h.requests().len(), 3);
    fs::remove_dir(&path).unwrap();
    fs::write(&path, "broken parent settings").unwrap();
    fs::create_dir(h.root.join("child")).unwrap();
    success(
        &h.command(&["--json", "project", "list"])
            .current_dir(h.root.join("child"))
            .output()
            .unwrap(),
    );
}

#[test]
fn xdg_fallback_profile_selection_and_missing_configuration_are_explicit() {
    let h = Harness::new(json!({"body":"[]"}));
    fs::create_dir_all(h.root.join(".config/backlog")).unwrap();
    fs::copy(
        h.root.join("backlog/config.json"),
        h.root.join(".config/backlog/config.json"),
    )
    .unwrap();
    success(
        &h.command(&["--json", "project", "list"])
            .env_remove("XDG_CONFIG_HOME")
            .output()
            .unwrap(),
    );
    for xdg in ["", "relative"] {
        success(
            &h.command(&["--json", "project", "list"])
                .env("XDG_CONFIG_HOME", xdg)
                .output()
                .unwrap(),
        );
    }
    h.config(json!({"profiles":{"default":{"spaceUrl":h.origin,"apiKeyEnv":"MISSING"}, "work":{"spaceUrl":format!("{}/",h.origin),"apiKeyEnv":"WORK_KEY"}}}));
    failure(&h.run(&["--json", "project", "list"]), 1, "config");
    failure(
        &h.run(&["--json", "project", "list", "--profile", "absent"]),
        1,
        "config",
    );
    failure(
        &h.command(&["--json", "project", "list", "--profile", "work"])
            .env("WORK_KEY", "")
            .output()
            .unwrap(),
        1,
        "config",
    );
    failure(
        &h.command(&["--json", "project", "list", "--profile", "work"])
            .env("WORK_KEY", "bad\nheader")
            .output()
            .unwrap(),
        1,
        "config",
    );
    success(
        &h.command(&["--profile", "work", "project", "list", "--json"])
            .env("WORK_KEY", "work-secret")
            .output()
            .unwrap(),
    );
    assert!(
        h.requests().last().unwrap()["headers"]
            .as_object()
            .unwrap()
            .iter()
            .any(|(name, value)| name.eq_ignore_ascii_case("Backlog-API-Key")
                && value == "work-secret")
    );
    fs::remove_file(h.root.join("backlog/config.json")).unwrap();
    let absent = failure(&h.run(&["--json", "project", "list"]), 1, "config");
    fs::write(h.root.join("backlog/config.json"), "broken").unwrap();
    let broken = failure(&h.run(&["--json", "project", "list"]), 1, "config");
    assert_ne!(absent["error"]["message"], broken["error"]["message"]);
    fs::remove_file(h.root.join("backlog/config.json")).unwrap();
    fs::create_dir(h.root.join("backlog/config.json")).unwrap();
    let unreadable = failure(&h.run(&["--json", "project", "list"]), 1, "config");
    assert_ne!(absent["error"]["message"], unreadable["error"]["message"]);
}

#[test]
fn editor_is_validated_without_execution_and_visual_editor_are_ignored() {
    use std::os::unix::fs::PermissionsExt;
    let h = Harness::new(json!({"body":"[]"}));
    let script = h.root.join("editor");
    fs::write(&script, "#!/bin/sh\ntouch editor-was-run\n").unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
    for program in ["./editor", "editor"] {
        h.config(json!({"editor":[program,"", "--wait"],"profiles":{"default":{"spaceUrl":h.origin,"apiKeyEnv":"TEST_KEY"}}}));
        success(
            &h.command(&["--json", "project", "list"])
                .env("PATH", "")
                .output()
                .unwrap(),
        );
    }
    assert!(!h.root.join("editor-was-run").exists());
    h.config(json!({"profiles":{"default":{"spaceUrl":h.origin,"apiKeyEnv":"TEST_KEY"}}}));
    success(
        &h.command(&["--json", "project", "list"])
            .env("VISUAL", "missing")
            .env("EDITOR", "missing")
            .output()
            .unwrap(),
    );
    failure(
        &h.command(&["--json", "project", "list"])
            .env("PATH", "")
            .output()
            .unwrap(),
        1,
        "config",
    );
}

#[test]
fn https_uses_private_temporary_credentials_and_removes_them_after_use() {
    use std::os::unix::fs::PermissionsExt;

    let h = Harness::new(json!({"body":"[]"}));
    let root = h.root.clone();
    let tls = root.join("tls");
    let certificate = fs::read(tls.join("localhost-cert.pem")).unwrap();
    for path in [&root, &tls, &tls.join("localhost-key.pem")] {
        assert_eq!(fs::metadata(path).unwrap().permissions().mode() & 0o077, 0);
    }
    for _ in 0..2 {
        assert_eq!(
            success(&h.run(&["--json", "project", "list"])),
            json!({"items":[]})
        );
    }
    assert_eq!(h.requests().len(), 2);
    assert_eq!(
        fs::read(tls.join("localhost-cert.pem")).unwrap(),
        certificate
    );
    drop(h);
    assert!(!root.exists());
}

#[test]
fn https_certificate_verification_is_not_bypassed() {
    let h = Harness::new(json!({"body":"[]"}));
    failure(
        &h.command(&["--json", "project", "list"])
            .env_remove("SSL_CERT_FILE")
            .output()
            .unwrap(),
        1,
        "network",
    );
    assert!(h.requests().is_empty());
}

#[test]
fn recognizable_json_flag_value_errors_still_use_json_but_arguments_after_separator_do_not() {
    let h = Harness::new(json!({"body":"[]"}));
    failure(&h.run(&["project", "list", "--json=true"]), 2, "input");
    let output = h.run(&["project", "list", "--", "--json"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .starts_with("error: ")
    );
}

#[test]
fn fixed_display_does_not_look_up_fields_by_redacted_keys() {
    let h = Harness::new(json!({"body":r#"[{"id":1,"projectKey":"P","name":"name"}]"#}));
    let output = h
        .command(&["project", "list"])
        .env("TEST_KEY", "name")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(output.stdout, b"ID\tKEY\tNAME\n1\tP\t[REDACTED]\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn output_failures_keep_observed_http_status_and_never_claim_empty_success() {
    let h = Harness::new(
        json!({"body":r#"[{"id":1,"projectKey":"P","name":"N","test-secret":1,"[REDACTED]":2}]"#}),
    );
    let error = failure(&h.run(&["--json", "project", "list"]), 1, "output");
    assert_eq!(error["error"]["httpStatus"], 200);
    fs::write(
        h.root.join("response.json"),
        json!({"status":401,"body":r#"{"errors":[{"test-secret":1,"[REDACTED]":2}]}"#}).to_string(),
    )
    .unwrap();
    let error = failure(&h.run(&["--json", "project", "list"]), 1, "output");
    assert_eq!(error["error"]["httpStatus"], 401);
    fs::write(
        h.root.join("response.json"),
        json!({"body":"[]","delay":0.2}).to_string(),
    )
    .unwrap();
    let mut child = h
        .command(&["--json", "project", "list"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    let error = failure(&child.wait_with_output().unwrap(), 1, "output");
    assert_eq!(error["error"]["httpStatus"], 200);
}

#[test]
fn tty_and_no_color_do_not_enable_prompts_or_terminal_escapes() {
    let h = Harness::new(
        json!({"body":r#"[{"id":1,"projectKey":"P","name":"test-secret\u001b[31m"}]"#}),
    );
    let template = h.command(&[]);
    for args in [
        vec!["project", "list"],
        vec!["project", "list", "--no-input"],
        vec!["--json", "project", "list"],
    ] {
        let output = Command::new("python3")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/tty_cli.py"
            ))
            .arg(env!("CARGO_BIN_EXE_bl-cli"))
            .args(&args)
            .env_clear()
            .envs(
                template
                    .get_envs()
                    .filter_map(|(key, value)| value.map(|v| (key, v))),
            )
            .env("PATH", std::env::var_os("PATH").unwrap())
            .current_dir(&h.root)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(!text.contains('\u{1b}'));
        assert!(!text.contains("test-secret"));
        if args.contains(&"--json") {
            let value: Value = serde_json::from_str(&text).unwrap();
            assert!(value["items"].is_array());
        } else {
            assert_eq!(
                text.split_whitespace().take(5).collect::<Vec<_>>(),
                ["ID", "KEY", "NAME", "1", "P"]
            );
            assert!(text.contains("[REDACTED]"));
        }
    }
    assert_eq!(h.requests().len(), 3);
}

#[test]
fn single_global_flags_cannot_be_repeated_across_subcommand_levels() {
    let h = Harness::new(json!({"body":"[]"}));
    failure(
        &h.run(&[
            "--profile",
            "default",
            "project",
            "list",
            "--profile=default",
            "--json",
        ]),
        2,
        "input",
    );
    failure(&h.run(&["--json", "project", "list", "--json"]), 2, "input");
    failure(
        &h.run(&["--no-input", "project", "list", "--no-input", "--json"]),
        2,
        "input",
    );
    assert!(h.requests().is_empty());
}

#[test]
fn command_help_and_version_bypass_even_broken_configuration() {
    let h = Harness::new(json!({"body":"[]"}));
    fs::write(h.root.join("backlog/config.json"), "broken").unwrap();
    for args in [
        vec!["project", "--help"],
        vec!["project", "list", "--help", "--json"],
        vec!["--json", "--version"],
    ] {
        let output = h.run(&args);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert!(output.stderr.is_empty());
        assert!(!output.stdout.is_empty());
        assert!(!output.stdout.contains(&0x1b));
    }
    assert!(h.requests().is_empty());
}

#[test]
fn help_and_version_output_failures_are_execution_errors_without_configuration_or_http() {
    let h = Harness::new(json!({"body":"[]"}));
    fs::write(h.root.join("backlog/config.json"), "broken").unwrap();
    for flag in ["--help", "--version"] {
        for json_mode in [false, true] {
            let args = if json_mode {
                vec!["--json", flag]
            } else {
                vec![flag]
            };
            let mut child = h
                .command(&args)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            drop(child.stdout.take());
            let output = child.wait_with_output().unwrap();
            if json_mode {
                let error = failure(&output, 1, "output");
                assert!(error["error"]["httpStatus"].is_null());
            } else {
                assert_eq!(output.status.code(), Some(1), "{output:?}");
                assert!(output.stdout.is_empty());
                assert_eq!(output.stderr, b"error: Cannot write standard output\n");
            }
        }
    }
    assert!(h.requests().is_empty());
}

#[test]
fn dangling_settings_symlinks_are_read_failures_not_absent_configuration() {
    use std::os::unix::fs::symlink;
    let h = Harness::new(json!({"body":"[]"}));
    fs::create_dir(h.root.join(".backlog")).unwrap();
    symlink("missing.json", h.root.join(".backlog/settings.json")).unwrap();
    failure(&h.run(&["--json", "project", "list"]), 1, "config");
    assert!(h.requests().is_empty());
}
