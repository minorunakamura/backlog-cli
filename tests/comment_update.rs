mod common;
use common::{Harness, success};
use serde_json::{Value, json};

fn args() -> Vec<&'static str> {
    vec!["issue", "comment", "update", "P-1", "7", "--json"]
}
fn comment() -> Value {
    json!({"id":7,"content":"old","createdUser":{"id":123456789012345678901234567890_u128,"name":"author"},"created":"created","updated":"updated","changeLog":null})
}
fn fixture(patch: Value) -> Value {
    json!({"routes":{
        "GET /api/v2/issues/P-1/comments/7":{"body":comment().to_string()},
        "GET /api/v2/users/myself":{"body":r#"{"id":123456789012345678901234567890}"#},
        "PATCH /api/v2/issues/P-1/comments/7":patch
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
        json!({"operation":"comment.update","outcome":outcome}),
        "{value}"
    );
    assert_eq!(value["error"]["httpStatus"], json!(status), "{value}");
    assert!(value["error"]["apiErrors"].is_array());
    assert_eq!(output.stderr.last(), Some(&b'\n'));
    value
}

#[test]
fn own_comment_patches_only_original_content_once_and_preserves_response_object() {
    let mut updated = comment();
    updated["content"] = json!(" 原文\n");
    updated["unknown"] = json!({"huge":123456789012345678901234567890_u128});
    updated["test-secret"] = json!("test-secret");
    let h = Harness::new(fixture(json!({"body":updated.to_string()})));
    std::fs::create_dir(h.root.join(".backlog")).unwrap();
    std::fs::write(h.root.join(".backlog/settings.json"), r#"{"project":"WRONG","issue":{"create":{"assignee":"unused","priority":"unused","issueType":"unused"}}}"#).unwrap();
    let raw = " \n日本語 e\u{301}👩‍💻 &+= test-secret\u{1b}\t\n ";
    let mut argv = args();
    argv.extend(["-b", raw]);
    let value = success(&h.run(&argv));
    assert_eq!(value["content"], " 原文\n");
    assert_eq!(
        value["unknown"]["huge"].to_string(),
        "123456789012345678901234567890"
    );
    assert_eq!(
        value["createdUser"]["id"].to_string(),
        "123456789012345678901234567890"
    );
    assert_eq!(value["[REDACTED]"], "[REDACTED]");
    assert!(value.get("resource").is_none());
    assert!(value.get("url").is_none());
    let r = h.requests();
    assert_eq!(
        r.len(),
        3,
        "latest comment, authenticated user, single PATCH only"
    );
    assert_eq!(r[0]["method"], "GET");
    assert_eq!(r[0]["path"], "/api/v2/issues/P-1/comments/7");
    assert_eq!(r[1]["method"], "GET");
    assert_eq!(r[1]["path"], "/api/v2/users/myself");
    assert_eq!(r[2]["method"], "PATCH");
    assert_eq!(r[2]["path"], "/api/v2/issues/P-1/comments/7");
    assert_eq!(r[2]["form"], json!({"content":[raw]}));
    assert!(
        r[2]["headers"]["content-type"]
            .as_str()
            .unwrap()
            .starts_with("application/x-www-form-urlencoded")
    );
    assert!(
        r.iter()
            .all(|r| r["headers"]["backlog-api-key"] == "test-secret")
    );
}

#[test]
fn leading_zero_comment_id_updates_the_same_numeric_id_with_one_content_patch() {
    let mut updated = comment();
    updated["content"] = json!("new");
    let h = Harness::new(json!({"routes":{
        "GET /api/v2/issues/P-1/comments/0007":{"body":comment().to_string()},
        "GET /api/v2/users/myself":{"body":r#"{"id":123456789012345678901234567890}"#},
        "PATCH /api/v2/issues/P-1/comments/0007":{"body":updated.to_string()}
    }}));
    let value = success(&h.run(&[
        "issue", "comment", "update", "P-1", "0007", "-b", "new", "--json",
    ]));
    assert_eq!(value["id"], 7);
    assert_eq!(value["content"], "new");
    let r = h.requests();
    assert_eq!(
        r.len(),
        3,
        "latest comment, myself, single PATCH; no retry or followup GET"
    );
    assert_eq!(r[0]["method"], "GET");
    assert_eq!(r[0]["path"], "/api/v2/issues/P-1/comments/0007");
    assert_eq!(r[1]["method"], "GET");
    assert_eq!(r[1]["path"], "/api/v2/users/myself");
    assert_eq!(r[2]["method"], "PATCH");
    assert_eq!(r[2]["path"], "/api/v2/issues/P-1/comments/0007");
    assert_eq!(r[2]["form"], json!({"content":["new"]}));
}

#[test]
fn only_own_non_null_changed_body_can_be_edited_and_owner_id_is_validated() {
    let h = Harness::new(json!({}));
    for (owner, body, kind, code, status) in [
        (json!(42), json!("old"), "input", 2, None),
        (
            comment()["createdUser"]["id"].clone(),
            Value::Null,
            "input",
            2,
            None,
        ),
        (
            comment()["createdUser"]["id"].clone(),
            json!("new"),
            "input",
            2,
            None,
        ),
        (Value::Null, json!("old"), "invalid_response", 1, Some(200)),
        (json!(0), json!("old"), "invalid_response", 1, Some(200)),
        (json!(-1), json!("old"), "invalid_response", 1, Some(200)),
        (json!(1.5), json!("old"), "invalid_response", 1, Some(200)),
        (
            json!("123456789012345678901234567890"),
            json!("old"),
            "invalid_response",
            1,
            Some(200),
        ),
    ] {
        let mut response = fixture(json!({"body":comment().to_string()}));
        let mut latest = comment();
        latest["createdUser"]["id"] = owner;
        latest["content"] = body;
        response["routes"]["GET /api/v2/issues/P-1/comments/7"]["body"] = json!(latest.to_string());
        std::fs::write(h.root.join("response.json"), response.to_string()).unwrap();
        let mut argv = args();
        argv.extend(["-b", "new"]);
        failure(&h.run(&argv), code, kind, "not_updated", status);
    }
    assert!(h.requests().iter().all(|r| r["method"] == "GET"));
    let mut response = fixture(json!({"body":comment().to_string()}));
    let mut latest = comment();
    latest["createdUser"].as_object_mut().unwrap().remove("id");
    response["routes"]["GET /api/v2/issues/P-1/comments/7"]["body"] = json!(latest.to_string());
    std::fs::write(h.root.join("response.json"), response.to_string()).unwrap();
    let mut argv = args();
    argv.extend(["-b", "new"]);
    failure(
        &h.run(&argv),
        1,
        "invalid_response",
        "not_updated",
        Some(200),
    );
    assert!(h.requests().iter().all(|r| r["method"] == "GET"));
}

#[test]
fn invalid_or_missing_body_identifiers_flags_and_config_never_send() {
    let h = Harness::new(fixture(json!({"body":comment().to_string()})));
    for changes in [
        vec!["-b", ""],
        vec![],
        vec!["-b", "", "--body-file", ""],
        vec!["--body", "one", "--body", "two"],
        vec!["--body-file", "one", "--body-file", "two"],
        vec!["-b", "new", "--json"],
        vec!["-b", "new", "--no-input", "--no-input"],
    ] {
        let mut argv = args();
        argv.extend(changes);
        failure(&h.run(&argv), 2, "input", "not_updated", None);
    }
    for flag in [
        "--project",
        "--title",
        "--status",
        "--status-id",
        "--assignee",
        "--assignee-id",
        "--resolution",
        "--resolution-id",
        "--issue-type",
        "--issue-type-id",
        "--priority",
        "--priority-id",
        "--milestone",
        "--milestone-id",
        "--start-date",
        "--due-date",
    ] {
        let mut argv = args();
        argv.extend(["-b", "new", flag, "1"]);
        failure(&h.run(&argv), 2, "input", "not_updated", None);
    }
    for flag in [
        "--clear-assignee",
        "--clear-resolution",
        "--clear-start-date",
        "--clear-due-date",
        "--clear-milestones",
    ] {
        let mut argv = args();
        argv.extend(["-b", "new", flag]);
        failure(&h.run(&argv), 2, "input", "not_updated", None);
    }
    for (issue, id) in [
        ("", "7"),
        ("0", "7"),
        ("P-0", "7"),
        ("../P-1", "7"),
        ("P-1", "0"),
        ("P-1", "-1"),
        ("P-1", "1.2"),
        ("P-1", "1e2"),
    ] {
        failure(
            &h.run(&[
                "issue", "comment", "update", issue, id, "-b", "new", "--json",
            ]),
            2,
            "input",
            "not_updated",
            None,
        );
    }
    for argv in [
        vec!["issue", "comment", "update", "--json"],
        vec!["issue", "comment", "update", "P-1", "--json"],
    ] {
        failure(&h.run(&argv), 2, "input", "not_updated", None);
    }
    for argv in [
        vec!["issue", "list", "issue", "comment", "update", "--json"],
        vec!["--profile", "issue", "comment", "update", "--json"],
    ] {
        common::failure(&h.run(&argv), 2, "input");
    }
    std::fs::write(h.root.join("bad.txt"), [0xff]).unwrap();
    std::fs::write(h.root.join("empty.txt"), "").unwrap();
    for path in ["bad.txt", "empty.txt", "missing.txt"] {
        let mut argv = args();
        argv.extend(["--body-file", path]);
        failure(&h.run(&argv), 2, "input", "not_updated", None);
    }
    assert!(h.requests().is_empty());
    std::fs::write(h.root.join("backlog/config.json"), "broken").unwrap();
    let mut argv = args();
    argv.extend(["-b", "new"]);
    failure(&h.run(&argv), 1, "config", "not_updated", None);
    let help = h.run(&["issue", "comment", "update", "--help"]);
    assert_eq!(help.status.code(), Some(0));
    let text = String::from_utf8(help.stdout).unwrap();
    for flag in ["--body", "--body-file", "<ISSUE_ID_OR_KEY>", "<COMMENT_ID>"] {
        assert!(text.contains(flag), "{text}");
    }
    assert!(!text.contains("--project"));
}

#[test]
fn patch_certainty_respects_api_permission_and_retains_updated_after_response_failures() {
    let h = Harness::new(json!({}));
    let mut invalid = comment();
    invalid["id"] = json!(0);
    for (patch, kind, outcome, status) in [
        (
            json!({"body":"{}"}),
            "invalid_response",
            "updated",
            Some(200),
        ),
        (
            json!({"body":invalid.to_string()}),
            "invalid_response",
            "updated",
            Some(200),
        ),
        (
            json!({"body":"not json"}),
            "invalid_response",
            "updated",
            Some(200),
        ),
        (
            json!({"body":"{}","truncate":true}),
            "network",
            "updated",
            Some(200),
        ),
        (json!({"disconnect":true}), "network", "unknown", None),
        (
            json!({"status":400,"body":r#"{"errors":[{"code":7,"message":"bad test-secret\u001b"}]}"#}),
            "api",
            "not_updated",
            Some(400),
        ),
        (
            json!({"status":401,"body":r#"{"errors":[{"code":11,"message":"bad"}]}"#}),
            "auth",
            "not_updated",
            Some(401),
        ),
        (
            json!({"status":403,"body":r#"{"errors":[{"code":5,"message":"denied"}]}"#}),
            "permission",
            "not_updated",
            Some(403),
        ),
        (
            json!({"status":404,"body":r#"{"errors":[{"code":6,"message":"missing"}]}"#}),
            "not_found",
            "not_updated",
            Some(404),
        ),
        (
            json!({"status":429,"body":r#"{"errors":[{"code":13,"message":"busy"}]}"#}),
            "rate_limit",
            "not_updated",
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
            json!({"status":201,"body":comment().to_string()}),
            "api",
            "unknown",
            Some(201),
        ),
    ] {
        std::fs::write(h.root.join("response.json"), fixture(patch).to_string()).unwrap();
        let mut argv = args();
        argv.extend(["-b", "new"]);
        let count = h.requests().len();
        let error = failure(&h.run(&argv), 1, kind, outcome, status);
        assert!(!error.to_string().contains("test-secret"));
        let r = h.requests();
        assert_eq!(
            r.len() - count,
            3,
            "no fallback, retry, redirect, confirmation GET"
        );
        assert_eq!(r[count + 2]["method"], "PATCH");
        assert_eq!(r[count + 2]["path"], "/api/v2/issues/P-1/comments/7");
    }
}

#[test]
fn literal_file_and_explicit_stdin_preserve_whitespace_and_reject_empty_or_non_utf8() {
    use std::io::Write;
    use std::process::Stdio;
    let h = Harness::new(fixture(json!({"body":comment().to_string()})));
    for raw in [
        " \n# **日本語** e\u{301}👩‍💻 &+= test-secret\u{1b}\t\n ",
        "\n ",
        "",
    ] {
        std::fs::write(h.root.join("body.txt"), raw).unwrap();
        for source in [
            vec!["--body", raw],
            vec!["--body-file", "body.txt"],
            vec!["--body-file", "-"],
        ] {
            let mut argv = args();
            argv.extend(source.clone());
            let count = h.requests().len();
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
                // Keep stdin open: non-explicit sources must not wait for EOF.
                let _stdin = child.stdin.take().unwrap();
                let start = std::time::Instant::now();
                while child.try_wait().unwrap().is_none() {
                    if start.elapsed() > std::time::Duration::from_secs(5) {
                        let _ = child.kill();
                        panic!("read implicit stdin");
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }
            let output = child.wait_with_output().unwrap();
            if raw.is_empty() {
                failure(&output, 2, "input", "not_updated", None);
                assert_eq!(h.requests().len(), count);
            } else {
                success(&output);
                let r = h.requests();
                assert_eq!(r.len() - count, 3);
                assert_eq!(r.last().unwrap()["form"], json!({"content":[raw]}));
            }
        }
    }
    let mut argv = args();
    argv.extend(["--body-file", "-"]);
    let count = h.requests().len();
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
        "not_updated",
        None,
    );
    assert_eq!(h.requests().len(), count);
}

#[test]
fn latest_comment_and_authenticated_user_failures_never_patch() {
    let h = Harness::new(json!({}));
    for path in [
        "GET /api/v2/issues/P-1/comments/7",
        "GET /api/v2/users/myself",
    ] {
        for (preflight, kind, status) in [
            (json!({"disconnect":true}), "network", None),
            (json!({"status":403}), "permission", Some(403)),
            (json!({"status":401}), "auth", Some(401)),
            (json!({"status":404}), "not_found", Some(404)),
            (json!({"body":"not json"}), "invalid_response", Some(200)),
            (json!({"body":"{}"}), "invalid_response", Some(200)),
            (json!({"body":"{\"id\":0}"}), "invalid_response", Some(200)),
        ] {
            let mut response = fixture(json!({}));
            response["routes"][path] = preflight;
            std::fs::write(h.root.join("response.json"), response.to_string()).unwrap();
            let mut argv = args();
            argv.extend(["-b", "new"]);
            let count = h.requests().len();
            failure(&h.run(&argv), 1, kind, "not_updated", status);
            assert_eq!(
                h.requests().len() - count,
                if path.contains("comments") { 1 } else { 2 }
            );
        }
    }
    assert!(h.requests().iter().all(|r| r["method"] == "GET"));
    let mut argv = args();
    argv.extend(["-b", "new"]);
    let count = h.requests().len();
    failure(
        &h.command(&argv)
            .env_remove("SSL_CERT_FILE")
            .output()
            .unwrap(),
        1,
        "network",
        "not_updated",
        None,
    );
    assert_eq!(h.requests().len(), count);
}

#[test]
fn output_failures_keep_updated_and_fixed_success_only_prints_comment_id() {
    use std::process::Stdio;
    let mut updated = comment();
    updated["test-secret"] = json!(1);
    updated["[REDACTED]"] = json!(2);
    let h = Harness::new(fixture(json!({"body":updated.to_string()})));
    let mut argv = args();
    argv.extend(["-b", "new"]);
    failure(&h.run(&argv), 1, "output", "updated", Some(200));
    std::fs::write(
        h.root.join("response.json"),
        fixture(json!({"body":comment().to_string()})).to_string(),
    )
    .unwrap();
    let fixed: Vec<_> = argv
        .iter()
        .copied()
        .filter(|arg| *arg != "--json")
        .collect();
    let output = h.run(&fixed);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "UPDATED\t7\n");
    for argv in [argv, fixed] {
        let mut child = h
            .command(&argv)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        drop(child.stdout.take());
        let output = child.wait_with_output().unwrap();
        if argv.contains(&"--json") {
            failure(&output, 1, "output", "updated", Some(200));
        } else {
            assert_eq!(output.status.code(), Some(1));
            assert_eq!(
                String::from_utf8(output.stderr).unwrap(),
                "error: Cannot write standard output [comment.update: updated]\n"
            );
        }
    }
    assert_eq!(h.requests().len(), 12);
}

#[test]
fn interrupts_before_send_after_send_and_after_200_keep_observed_certainty() {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    for (route, response, ready, count, outcome, status) in [
        (
            "GET /api/v2/issues/P-1/comments/7",
            json!({"header_delay":32}),
            "requests",
            1,
            "not_updated",
            None,
        ),
        (
            "GET /api/v2/issues/P-1/comments/7",
            json!({"body":comment().to_string(),"delay":32}),
            "get-headers-sent",
            1,
            "not_updated",
            Some(200),
        ),
        (
            "GET /api/v2/users/myself",
            json!({"header_delay":32}),
            "requests",
            2,
            "not_updated",
            None,
        ),
        (
            "PATCH /api/v2/issues/P-1/comments/7",
            json!({"header_delay":32}),
            "requests",
            3,
            "unknown",
            None,
        ),
        (
            "PATCH /api/v2/issues/P-1/comments/7",
            json!({"body":"{}","delay":32}),
            "patch-headers-sent",
            3,
            "updated",
            Some(200),
        ),
        (
            "PATCH /api/v2/issues/P-1/comments/7",
            json!({"status":403,"body":r#"{"errors":[{"code":5,"message":"denied"}]}"#,"delay":32}),
            "patch-headers-sent",
            3,
            "unknown",
            Some(403),
        ),
    ] {
        let mut routes = fixture(json!({}));
        routes["routes"][route] = response;
        let h = Harness::new(routes);
        let mut argv = args();
        argv.extend(["-b", "new"]);
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
        assert_eq!(h.requests().len(), count, "no surviving task or retry");
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
        "not_updated",
        None,
    );
    assert!(h.requests().is_empty());
}

#[test]
fn global_profile_and_tty_remain_noninteractive() {
    let h = Harness::new(fixture(json!({"body":comment().to_string()})));
    h.config(json!({"profiles":{"work":{"spaceUrl":h.origin,"apiKeyEnv":"TEST_KEY"}}}));
    let mut argv = args();
    argv.extend(["-b", "new"]);
    argv.splice(0..0, ["--profile", "work", "--no-input"]);
    success(&h.run(&argv));
    let mut argv = args();
    argv.extend(["-b", "new", "--profile", "work", "--no-input"]);
    success(&h.run(&argv));
    let template = h.command(&[]);
    let output = std::process::Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/tty_cli.py"
        ))
        .arg(env!("CARGO_BIN_EXE_bl-cli"))
        .args([
            "issue",
            "comment",
            "update",
            "P-1",
            "7",
            "-b",
            "new",
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
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stderr.is_empty());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "UPDATED\t7\r\n");
    assert_eq!(h.requests().len(), 9);
}
