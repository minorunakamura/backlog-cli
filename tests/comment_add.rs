mod common;
use common::{Harness, success};
use serde_json::{Value, json};

fn args() -> Vec<&'static str> {
    vec!["issue", "comment", "add", "P-1", "--json"]
}
fn latest() -> Value {
    json!({"projectId":42,"status":{"id":3},"assignee":{"id":4},"resolution":null})
}
fn fixture(post: Value, patch: Value) -> Value {
    json!({"routes":{
        "GET /api/v2/issues/P-1":{"body":latest().to_string()},
        "POST /api/v2/issues/P-1/comments":post,
        "PATCH /api/v2/issues/P-1":patch
    }})
}
fn failure(
    output: &std::process::Output,
    code: i32,
    kind: &str,
    outcome: &str,
    status: Option<u16>,
) -> Value {
    assert_eq!(output.status.code(), Some(code), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    let value: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(value["error"]["kind"], kind, "{value}");
    assert_eq!(
        value["error"]["write"],
        json!({"operation":"comment.add","outcome":outcome}),
        "{value}"
    );
    assert_eq!(value["error"]["httpStatus"], json!(status), "{value}");
    assert_eq!(output.stderr.last(), Some(&b'\n'));
    value
}

#[test]
fn body_only_posts_original_utf8_once_and_preserves_raw_comment_json() {
    let h = Harness::new(fixture(
        json!({"status":201,"body":r#"{"id":123456789012345678901234567890,"content":" 原文\n","unknown":{"huge":123456789012345678901234567890},"test-secret":"test-secret"}"#}),
        json!({}),
    ));
    let raw = " \n日本語 e\u{301}👩‍💻 &+= test-secret\u{1b}\t\n ";
    let mut argv = args();
    argv.extend(["-b", raw]);
    let value = success(&h.run(&argv));
    assert_eq!(value["resource"], "comment");
    assert_eq!(
        value["data"]["id"].to_string(),
        "123456789012345678901234567890"
    );
    assert_eq!(
        value["data"]["unknown"]["huge"].to_string(),
        "123456789012345678901234567890"
    );
    assert_eq!(value["data"]["content"], " 原文\n");
    assert_eq!(value["data"]["[REDACTED]"], "[REDACTED]");
    let r = h.requests();
    assert_eq!(r.len(), 1, "no lookup, retry, or comment ID completion GET");
    assert_eq!(r[0]["method"], "POST");
    assert_eq!(r[0]["path"], "/api/v2/issues/P-1/comments");
    assert_eq!(r[0]["headers"]["backlog-api-key"], "test-secret");
    assert!(
        r[0]["headers"]["content-type"]
            .as_str()
            .unwrap()
            .starts_with("application/x-www-form-urlencoded")
    );
    assert_eq!(r[0]["form"], json!({"content":[raw]}));
}

#[test]
fn changed_fields_patch_once_with_optional_comment_and_raw_issue_without_url_added() {
    let h = Harness::new(fixture(
        json!({}),
        json!({"body":r#"{"id":123456789012345678901234567890,"issueKey":"P-1","unknown":{"raw":" 原文\n","huge":123456789012345678901234567890},"url":"original"}"#}),
    ));
    std::fs::create_dir(h.root.join(".backlog")).unwrap();
    std::fs::write(h.root.join(".backlog/settings.json"), r#"{"project":"WRONG","issue":{"create":{"assignee":"unused","priority":"unused","issueType":"unused"}}}"#).unwrap();
    for body in [Some(" \nコメント &+=\n "), Some(""), None] {
        let mut argv = args();
        argv.extend([
            "--status-id",
            "008",
            "--clear-assignee",
            "--resolution-id",
            "000",
        ]);
        if let Some(body) = body {
            argv.extend(["--body", body]);
        }
        let value = success(&h.run(&argv));
        assert_eq!(value["resource"], "issue");
        assert_eq!(value["data"]["url"], "original");
        assert_eq!(
            value["data"]["unknown"]["huge"].to_string(),
            "123456789012345678901234567890"
        );
        assert_eq!(value["data"]["unknown"]["raw"], " 原文\n");
        let r = h.requests();
        let write = r.last().unwrap();
        assert_eq!(write["method"], "PATCH");
        assert_eq!(write["path"], "/api/v2/issues/P-1");
        assert_eq!(write["headers"]["backlog-api-key"], "test-secret");
        let mut expected = json!({"statusId":["8"],"assigneeId":[""],"resolutionId":["0"]});
        if let Some(body) = body.filter(|b| !b.is_empty()) {
            expected["comment"] = json!([body]);
        }
        assert_eq!(write["form"], expected);
    }
    assert_eq!(
        h.requests().len(),
        6,
        "GET latest + PATCH only, no fallback or ID completion"
    );
}

#[test]
fn same_fields_choose_post_or_reject_without_body_and_clear_zero_is_a_real_change() {
    let mut response = fixture(
        json!({"status":201,"body":"{\"id\":9}"}),
        json!({"body":"{\"id\":1,\"issueKey\":\"P-1\"}"}),
    );
    let mut current = latest();
    current["resolution"] = json!({"id":0});
    response["routes"]["GET /api/v2/issues/P-1"]["body"] = json!(current.to_string());
    let h = Harness::new(response);
    let mut argv = args();
    argv.extend([
        "--status-id",
        "003",
        "--assignee-id",
        "004",
        "--resolution-id",
        "000",
    ]);
    for body in [None, Some("")] {
        let mut argv = argv.clone();
        if let Some(body) = body {
            argv.extend(["--body", body]);
        }
        failure(&h.run(&argv), 2, "input", "not_created", None);
    }
    argv.extend(["-b", "\n "]);
    assert_eq!(success(&h.run(&argv))["resource"], "comment");
    assert_eq!(
        h.requests().last().unwrap()["form"],
        json!({"content":["\n "]})
    );
    success(&h.run(&[
        "issue",
        "comment",
        "add",
        "P-1",
        "--clear-resolution",
        "--json",
    ]));
    assert_eq!(
        h.requests().last().unwrap()["form"],
        json!({"resolutionId":[""]})
    );
    assert_eq!(h.requests().len(), 6);
    let mut current = latest();
    current["assignee"] = Value::Null;
    let mut response = fixture(json!({"status":201,"body":"{\"id\":9}"}), json!({}));
    response["routes"]["GET /api/v2/issues/P-1"]["body"] = json!(current.to_string());
    std::fs::write(h.root.join("response.json"), response.to_string()).unwrap();
    success(&h.run(&[
        "issue",
        "comment",
        "add",
        "P-1",
        "--clear-assignee",
        "--clear-resolution",
        "-b",
        "body",
        "--json",
    ]));
    assert_eq!(
        h.requests().last().unwrap()["form"],
        json!({"content":["body"]})
    );
    assert_eq!(h.requests().last().unwrap()["method"], "POST");
    let count = h.requests().len();
    for body in [vec![], vec!["--body", ""]] {
        let mut argv = args();
        argv.extend(body);
        failure(&h.run(&argv), 2, "input", "not_created", None);
    }
    assert_eq!(h.requests().len(), count, "empty operation needs no HTTP");
}

#[test]
fn names_use_issue_project_and_self_without_defaults_or_automatic_resolution() {
    let mut response = fixture(json!({}), json!({"body":"{\"id\":1,\"issueKey\":\"P-1\"}"}));
    for (path, body) in [
        (
            "/api/v2/projects/42/statuses",
            r#"[{"id":8,"name":"完了"}]"#,
        ),
        ("/api/v2/projects/42/users", r#"[{"id":9,"name":"担当"}]"#),
        ("/api/v2/resolutions", r#"[{"id":0,"name":"対応済み"}]"#),
        ("/api/v2/users/myself", r#"{"id":10}"#),
    ] {
        response["routes"][path] = json!({"body":body});
    }
    let h = Harness::new(response);
    std::fs::create_dir(h.root.join(".backlog")).unwrap();
    std::fs::write(
        h.root.join(".backlog/settings.json"),
        r#"{"project":"WRONG","issue":{"create":{"assignee":"unused"}}}"#,
    )
    .unwrap();
    let mut argv = args();
    argv.extend(["-s", "完了", "-a", "担当", "--resolution", "対応済み"]);
    success(&h.run(&argv));
    assert_eq!(h.requests().len(), 5);
    assert_eq!(
        h.requests().last().unwrap()["form"],
        json!({"statusId":["8"],"assigneeId":["9"],"resolutionId":["0"]})
    );
    let mut argv = args();
    argv.extend(["-a", "self"]);
    success(&h.run(&argv));
    let r = h.requests();
    assert_eq!(r.len(), 8);
    assert_eq!(r[6]["path"], "/api/v2/users/myself");
    assert_eq!(r[7]["form"], json!({"assigneeId":["10"]}));
    let mut response = fixture(json!({}), json!({"body":"{\"id\":1,\"issueKey\":\"P-1\"}"}));
    let mut current = latest();
    current["resolution"] = json!({"id":0});
    response["routes"]["GET /api/v2/issues/P-1"]["body"] = json!(current.to_string());
    std::fs::write(h.root.join("response.json"), response.to_string()).unwrap();
    let mut argv = args();
    argv.extend(["--status-id", "8"]);
    success(&h.run(&argv));
    assert_eq!(
        h.requests().last().unwrap()["form"],
        json!({"statusId":["8"]}),
        "no resolution selected or cleared automatically"
    );
}

#[test]
fn input_config_and_body_errors_identify_comment_add_before_any_http() {
    let h = Harness::new(json!({}));
    for changes in [
        vec!["--status", "open", "--status-id", "3"],
        vec!["-a", "self", "--assignee-id", "4"],
        vec!["--resolution", "done", "--resolution-id", "0"],
        vec!["--clear-assignee", "-a", "self"],
        vec!["--clear-assignee", "--assignee-id", "4"],
        vec!["--clear-resolution", "--resolution", "done"],
        vec!["--clear-resolution", "--resolution-id", "0"],
        vec!["--body", "", "--body-file", ""],
        vec!["--status-id", "0"],
        vec!["--assignee-id", "1.2"],
        vec!["--resolution-id", "1e2"],
        vec!["--project", "42"],
        vec!["--title", "new"],
        vec!["--issue-type-id", "1"],
        vec!["--priority-id", "1"],
        vec!["--milestone-id", "1"],
        vec!["--start-date", "2026-01-01"],
        vec!["--due-date", "2026-01-01"],
        vec!["--clear-due-date"],
        vec!["--clear-milestones"],
    ] {
        let mut argv = args();
        argv.extend(changes);
        failure(&h.run(&argv), 2, "input", "not_created", None);
    }
    for flag in [
        "--body",
        "--body-file",
        "--status",
        "--status-id",
        "--assignee",
        "--assignee-id",
        "--resolution",
        "--resolution-id",
    ] {
        let mut argv = args();
        argv.extend([flag, "1", flag, "2"]);
        failure(&h.run(&argv), 2, "input", "not_created", None);
    }
    for flag in [
        "--clear-assignee",
        "--clear-resolution",
        "--json",
        "--no-input",
    ] {
        let mut argv = args();
        argv.extend([flag, flag]);
        failure(&h.run(&argv), 2, "input", "not_created", None);
    }
    for issue in ["", "0", "P-0", "../P-1"] {
        failure(
            &h.run(&["issue", "comment", "add", issue, "-b", "body", "--json"]),
            2,
            "input",
            "not_created",
            None,
        );
    }
    failure(
        &h.run(&["issue", "comment", "add", "--json"]),
        2,
        "input",
        "not_created",
        None,
    );
    for argv in [
        vec!["issue", "list", "issue", "comment", "add", "--json"],
        vec!["--profile", "issue", "comment", "add", "--json"],
    ] {
        common::failure(&h.run(&argv), 2, "input");
    }
    std::fs::write(h.root.join("bad.txt"), [0xff]).unwrap();
    for path in ["bad.txt", "missing.txt"] {
        let mut argv = args();
        argv.extend(["--body-file", path]);
        failure(&h.run(&argv), 2, "input", "not_created", None);
    }
    assert!(h.requests().is_empty());
    std::fs::write(h.root.join("backlog/config.json"), "broken").unwrap();
    failure(&h.run(&args()), 1, "config", "not_created", None);
    let help = h.run(&["issue", "comment", "add", "--help"]);
    assert_eq!(help.status.code(), Some(0));
    let text = String::from_utf8(help.stdout).unwrap();
    for flag in [
        "--status-id",
        "--resolution-id",
        "--clear-assignee",
        "--body-file",
        "<ISSUE_ID_OR_KEY>",
    ] {
        assert!(text.contains(flag), "{text}");
    }
    assert!(!text.contains("--project"));
}

#[test]
fn file_and_explicit_stdin_preserve_body_and_empty_means_no_comment() {
    use std::io::Write;
    use std::process::Stdio;
    let h = Harness::new(fixture(
        json!({"status":201,"body":"{\"id\":9}"}),
        json!({"body":"{\"id\":1,\"issueKey\":\"P-1\"}"}),
    ));
    for raw in [" \n# **日本語** e\u{301}👩‍💻 &+= test-secret\u{1b}\t\n ", ""] {
        std::fs::write(h.root.join("body.txt"), raw).unwrap();
        for source in [
            vec!["--body", raw],
            vec!["--body-file", "body.txt"],
            vec!["--body-file", "-"],
        ] {
            let mut argv = args();
            argv.extend(source.clone());
            if raw.is_empty() {
                argv.extend(["--status-id", "8"]);
            }
            let mut child = h
                .command(&argv)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            if source == ["--body-file", "-"] {
                child
                    .stdin
                    .take()
                    .unwrap()
                    .write_all(raw.as_bytes())
                    .unwrap();
            } else {
                drop(child.stdin.take());
            }
            success(&child.wait_with_output().unwrap());
            assert_eq!(
                h.requests().last().unwrap()["form"],
                if raw.is_empty() {
                    json!({"statusId":["8"]})
                } else {
                    json!({"content":[raw]})
                }
            );
        }
    }
    let mut argv = args();
    argv.extend(["--body-file", "-"]);
    let mut child = h
        .command(&argv)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(&[0xff]).unwrap();
    failure(
        &child.wait_with_output().unwrap(),
        2,
        "input",
        "not_created",
        None,
    );
    assert_eq!(h.requests().len(), 9);
    let mut argv = args();
    argv.extend(["--status-id", "8"]);
    let mut child = h
        .command(&argv)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let _open_stdin = child.stdin.take().unwrap();
    let start = std::time::Instant::now();
    while child.try_wait().unwrap().is_none() {
        if start.elapsed() > std::time::Duration::from_secs(5) {
            let _ = child.kill();
            panic!("read implicit stdin");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    success(&child.wait_with_output().unwrap());
}

#[test]
fn preflight_and_name_resolution_failures_never_write_and_use_unselected_fallback() {
    let h = Harness::new(json!({}));
    for (preflight, kind, status) in [
        (json!({"disconnect":true}), "network", None),
        (json!({"status":403}), "permission", Some(403)),
        (json!({"body":"not json"}), "invalid_response", Some(200)),
        (json!({"body":"{}"}), "invalid_response", Some(200)),
        (
            json!({"body":"{\"projectId\":42,\"status\":{\"id\":0}}"}),
            "invalid_response",
            Some(200),
        ),
    ] {
        std::fs::write(
            h.root.join("response.json"),
            json!({"routes":{"GET /api/v2/issues/P-1":preflight}}).to_string(),
        )
        .unwrap();
        let mut argv = args();
        argv.extend(["--status-id", "8", "-b", "body"]);
        failure(&h.run(&argv), 1, kind, "not_created", status);
    }
    for (body, kind, status) in [
        ("[]", "input", None),
        (
            r#"[{"id":8,"name":"done"},{"id":9,"name":"done"}]"#,
            "input",
            None,
        ),
        (r#"[{"name":"done"}]"#, "invalid_response", Some(200)),
    ] {
        let mut response = fixture(json!({}), json!({}));
        response["routes"]["/api/v2/projects/42/statuses"] = json!({"body":body});
        std::fs::write(h.root.join("response.json"), response.to_string()).unwrap();
        let mut argv = args();
        argv.extend(["--status", "done", "-b", "body"]);
        failure(
            &h.run(&argv),
            if kind == "input" { 2 } else { 1 },
            kind,
            "not_created",
            status,
        );
    }
    assert!(h.requests().iter().all(|r| r["method"] == "GET"));
}

#[test]
fn post_and_patch_certainty_follow_actual_method_without_permission_fallback() {
    let h = Harness::new(json!({}));
    for patch in [false, true] {
        let (success_status, written, not_written) = if patch {
            (200, "updated", "not_updated")
        } else {
            (201, "created", "not_created")
        };
        let cases = [
            (
                json!({"status":success_status,"body":"not json"}),
                "invalid_response",
                written,
                Some(success_status),
            ),
            (
                json!({"status":success_status,"body":"{}"}),
                "invalid_response",
                written,
                Some(success_status),
            ),
            (
                json!({"status":success_status,"body":"{\"id\":0,\"issueKey\":\"P-1\"}"}),
                "invalid_response",
                written,
                Some(success_status),
            ),
            (
                json!({"status":success_status,"body":"{}","truncate":true}),
                "network",
                written,
                Some(success_status),
            ),
            (json!({"disconnect":true}), "network", "unknown", None),
            (
                json!({"status":400,"body":r#"{"errors":[{"code":7,"message":"bad test-secret\u001b"}]}"#}),
                "api",
                not_written,
                Some(400),
            ),
            (
                json!({"status":401,"body":r#"{"errors":[{"code":11,"message":"bad"}]}"#}),
                "auth",
                not_written,
                Some(401),
            ),
            (
                json!({"status":403,"body":r#"{"errors":[{"code":5,"message":"denied"}]}"#}),
                "permission",
                not_written,
                Some(403),
            ),
            (
                json!({"status":404,"body":r#"{"errors":[{"code":6,"message":"missing"}]}"#}),
                "not_found",
                not_written,
                Some(404),
            ),
            (
                json!({"status":429,"body":r#"{"errors":[{"code":13,"message":"busy"}]}"#}),
                "rate_limit",
                not_written,
                Some(429),
            ),
            (
                json!({"status":403,"body":"{}"}),
                "permission",
                "unknown",
                Some(403),
            ),
            (
                json!({"status":400,"body":"{\"errors\":[{}]}"}),
                "api",
                "unknown",
                Some(400),
            ),
            (
                json!({"status":400,"body":"not json"}),
                "api",
                "unknown",
                Some(400),
            ),
            (
                json!({"status":500,"body":r#"{"errors":[{"code":1,"message":"bad"}]}"#}),
                "api",
                "unknown",
                Some(500),
            ),
            (
                json!({"status":302,"location":"https://unused.invalid"}),
                "api",
                "unknown",
                Some(302),
            ),
            (
                json!({"status":if patch {201} else {200},"body":"{\"id\":1,\"issueKey\":\"P-1\"}"}),
                "api",
                "unknown",
                Some(if patch { 201 } else { 200 }),
            ),
        ];
        let count = h.requests().len();
        for (response, kind, outcome, status) in cases {
            std::fs::write(
                h.root.join("response.json"),
                fixture(response.clone(), response).to_string(),
            )
            .unwrap();
            let mut argv = args();
            argv.extend(["-b", "body"]);
            if patch {
                argv.extend(["--status-id", "8"]);
            }
            let error = failure(&h.run(&argv), 1, kind, outcome, status);
            assert!(!error.to_string().contains("test-secret"));
        }
        let r = h.requests();
        assert_eq!(
            r.len() - count,
            16 * if patch { 2 } else { 1 },
            "no fallback, retry, redirect or confirmation GET"
        );
        assert_eq!(
            r[count..].iter().filter(|r| r["method"] != "GET").count(),
            16
        );
        assert!(
            r[count..].iter().all(
                |r| r["method"] == "GET" || r["method"] == if patch { "PATCH" } else { "POST" }
            )
        );
    }
}

#[test]
fn output_failures_keep_created_or_updated_and_success_fixed_display_needs_no_get() {
    use std::process::Stdio;
    let h = Harness::new(json!({}));
    for patch in [false, true] {
        let status = if patch { 200 } else { 201 };
        let outcome = if patch { "updated" } else { "created" };
        let mut argv = args();
        argv.extend(["-b", "body"]);
        if patch {
            argv.extend(["--status-id", "8"]);
        }
        let response = json!({"status":status,"body":r#"{"id":9,"issueKey":"P-1","test-secret":1,"[REDACTED]":2}"#});
        std::fs::write(
            h.root.join("response.json"),
            fixture(response.clone(), response).to_string(),
        )
        .unwrap();
        failure(&h.run(&argv), 1, "output", outcome, Some(status));
        let response =
            json!({"status":status,"body":r#"{"id":9,"issueKey":"P-1","raw":" 原文\n"}"#});
        std::fs::write(
            h.root.join("response.json"),
            fixture(response.clone(), response).to_string(),
        )
        .unwrap();
        let value = success(&h.run(&argv));
        assert!(
            value["data"].get("url").is_none(),
            "add must not add issue URLs"
        );
        let fixed: Vec<_> = argv
            .iter()
            .copied()
            .filter(|arg| *arg != "--json")
            .collect();
        let out = h.run(&fixed);
        assert_eq!(out.status.code(), Some(0));
        assert_eq!(
            String::from_utf8(out.stdout).unwrap(),
            if patch {
                "UPDATED\tP-1\n"
            } else {
                "CREATED\t9\n"
            }
        );
        assert!(out.stderr.is_empty());
        for argv in [argv, fixed] {
            let mut child = h
                .command(&argv)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            drop(child.stdout.take());
            let out = child.wait_with_output().unwrap();
            if argv.contains(&"--json") {
                failure(&out, 1, "output", outcome, Some(status));
            } else {
                assert_eq!(out.status.code(), Some(1));
                assert_eq!(
                    String::from_utf8(out.stderr).unwrap(),
                    format!("error: Cannot write standard output [comment.add: {outcome}]\n")
                );
            }
        }
    }
    assert_eq!(h.requests().len(), 15);
}

#[test]
fn interrupts_keep_unselected_send_and_success_header_certainty_for_both_methods() {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    for (patch, preflight, write, ready, count, outcome, status) in [
        (
            true,
            json!({"header_delay":32}),
            json!({}),
            "requests",
            1,
            "not_created",
            None,
        ),
        (
            true,
            json!({"body":latest().to_string(),"delay":32}),
            json!({}),
            "get-headers-sent",
            1,
            "not_created",
            Some(200),
        ),
        (
            false,
            json!({}),
            json!({"header_delay":32}),
            "requests",
            1,
            "unknown",
            None,
        ),
        (
            true,
            json!({"body":latest().to_string()}),
            json!({"header_delay":32}),
            "requests",
            2,
            "unknown",
            None,
        ),
        (
            false,
            json!({}),
            json!({"status":201,"body":"{}","delay":32}),
            "post-headers-sent",
            1,
            "created",
            Some(201),
        ),
        (
            true,
            json!({"body":latest().to_string()}),
            json!({"body":"{}","delay":32}),
            "patch-headers-sent",
            2,
            "updated",
            Some(200),
        ),
        (
            true,
            json!({"body":latest().to_string()}),
            json!({"status":403,"body":r#"{"errors":[{"code":5,"message":"denied"}]}"#,"delay":32}),
            "patch-headers-sent",
            2,
            "unknown",
            Some(403),
        ),
    ] {
        let mut response = fixture(write.clone(), write);
        response["routes"]["GET /api/v2/issues/P-1"] = preflight;
        let h = Harness::new(response);
        let mut argv = args();
        argv.extend(["-b", "body"]);
        if patch {
            argv.extend(["--status-id", "8"]);
        }
        let child = h
            .command(&argv)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let start = Instant::now();
        while !h.root.join(ready).exists() || h.requests().len() != count {
            assert!(start.elapsed() < Duration::from_secs(10));
            std::thread::sleep(Duration::from_millis(10));
        }
        if status.is_some() {
            std::thread::sleep(Duration::from_millis(500));
        }
        assert!(
            Command::new("/bin/kill")
                .args(["-INT", &child.id().to_string()])
                .status()
                .unwrap()
                .success()
        );
        failure(
            &child.wait_with_output().unwrap(),
            130,
            "interrupted",
            outcome,
            status,
        );
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(
            h.requests().len(),
            count,
            "no surviving task, retry or fallback"
        );
    }
}

#[test]
fn explicit_stdin_interrupt_never_fetches_or_writes() {
    use std::process::{Command, Stdio};
    let h = Harness::new(json!({}));
    let mut argv = args();
    argv.extend(["--body-file", "-"]);
    let mut child = h
        .command(&argv)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let _stdin = child.stdin.take().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1500));
    assert!(
        Command::new("/bin/kill")
            .args(["-INT", &child.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    failure(
        &child.wait_with_output().unwrap(),
        130,
        "interrupted",
        "not_created",
        None,
    );
    assert!(h.requests().is_empty());
}

#[test]
fn global_profile_flags_and_tty_remain_noninteractive() {
    let h = Harness::new(fixture(
        json!({"status":201,"body":"{\"id\":9}"}),
        json!({}),
    ));
    h.config(json!({"profiles":{"work":{"spaceUrl":h.origin,"apiKeyEnv":"TEST_KEY"}}}));
    let mut argv = args();
    argv.extend(["-b", "body"]);
    argv.splice(0..0, ["--profile", "work", "--no-input"]);
    success(&h.run(&argv));
    let mut argv = args();
    argv.extend(["-b", "body", "--profile", "work", "--no-input"]);
    success(&h.run(&argv));
    let template = h.command(&[]);
    let out = std::process::Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/tty_cli.py"
        ))
        .arg(env!("CARGO_BIN_EXE_bl-cli"))
        .args([
            "issue",
            "comment",
            "add",
            "P-1",
            "-b",
            "body",
            "--profile",
            "work",
            "--no-input",
        ])
        .env_clear()
        .envs(template.get_envs().filter_map(|(k, v)| v.map(|v| (k, v))))
        .env("PATH", std::env::var_os("PATH").unwrap())
        .current_dir(&h.root)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(out.stderr.is_empty());
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "CREATED\t9\r\n");
    assert_eq!(h.requests().len(), 3);
}

#[test]
fn post_connection_and_tls_failures_are_not_created_without_http_writes() {
    use std::net::TcpListener;
    let h = Harness::new(json!({}));
    let mut argv = args();
    argv.extend(["-b", "body"]);
    failure(
        &h.command(&argv)
            .env_remove("SSL_CERT_FILE")
            .output()
            .unwrap(),
        1,
        "network",
        "not_created",
        None,
    );
    assert!(h.requests().is_empty());
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    h.config(json!({"profiles":{"default":{"spaceUrl":format!("https://{address}"),"apiKeyEnv":"TEST_KEY"}}}));
    failure(&h.run(&argv), 1, "network", "not_created", None);
    assert!(h.requests().is_empty());
}
