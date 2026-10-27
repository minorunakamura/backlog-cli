mod common;
use common::{Harness, success};
use serde_json::json;

fn args() -> Vec<&'static str> {
    vec![
        "issue",
        "create",
        "-p",
        "123",
        "-t",
        " 修正 ",
        "--issue-type-id",
        "7",
        "--priority-id",
        "3",
        "--json",
    ]
}
#[test]
fn create_posts_one_raw_form_and_returns_object_with_safe_url() {
    let h = Harness::new(
        json!({"status":201,"body":r#"{"id":123456789012345678901234567890,"issueKey":"P-1","extra":{"raw":"  原文\n"}}"#}),
    );
    let mut argv = args();
    argv.extend([
        "-b",
        " \n# **原文** &+= test-secret\t\n",
        "--milestone-id",
        "007",
        "--milestone-id",
        "7",
        "--milestone-id",
        "8",
    ]);
    let value = success(&h.run(&argv));
    assert_eq!(value["url"], format!("{}/view/P-1", h.origin));
    assert_eq!(value["id"].to_string(), "123456789012345678901234567890");
    let requests = h.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0]["method"], "POST");
    assert_eq!(requests[0]["path"], "/api/v2/issues");
    assert_eq!(requests[0]["headers"]["backlog-api-key"], "test-secret");
    assert_eq!(
        requests[0]["form"],
        json!({"projectId":["123"],"summary":[" 修正 "],"issueTypeId":["7"],"priorityId":["3"],"description":[" \n# **原文** &+= test-secret\t\n"],"milestoneId[]":["7","8"]})
    );
}

fn failure(
    output: &std::process::Output,
    code: i32,
    kind: &str,
    outcome: &str,
    status: Option<u16>,
) -> serde_json::Value {
    assert_eq!(output.status.code(), Some(code), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    let value: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(value["error"]["kind"], kind, "{value}");
    assert_eq!(
        value["error"]["write"],
        json!({"operation":"issue.create","outcome":outcome}),
        "{value}"
    );
    assert_eq!(value["error"]["httpStatus"], json!(status));
    assert_eq!(output.stderr.last(), Some(&b'\n'));
    value
}
#[test]
fn create_tcp_connection_refusal_is_not_created() {
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};

    let h = Harness::new(json!({}));
    // Reserve the port without listening so another process cannot claim it during the test.
    let mut socket = Command::new("python3")
        .args([
            "-c",
            "import socket, sys; s = socket.socket(); s.bind(('127.0.0.1', 0)); print(s.getsockname()[1], flush=True); sys.stdin.read()",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut port = String::new();
    BufReader::new(socket.stdout.take().unwrap())
        .read_line(&mut port)
        .unwrap();
    let port: u16 = port.trim().parse().unwrap();
    h.config(json!({"profiles":{"default":{"spaceUrl":format!("https://127.0.0.1:{port}"),"apiKeyEnv":"TEST_KEY"}}}));
    let output = h.command(&args()).output();
    drop(socket.stdin.take());
    assert!(socket.wait().unwrap().success());
    failure(&output.unwrap(), 1, "network", "not_created", None);
    assert!(!h.root.join("requests").exists());
}

#[test]
fn create_untrusted_tls_failure_is_not_created_without_http_transmission() {
    let h = Harness::new(json!({"status":201,"body":"{\"id\":1,\"issueKey\":\"P-1\"}"}));
    failure(
        &h.command(&args())
            .env_remove("SSL_CERT_FILE")
            .output()
            .unwrap(),
        1,
        "network",
        "not_created",
        None,
    );
    assert!(!h.root.join("requests").exists());
}

#[test]
fn create_certainty_survives_invalid_response_and_trusts_only_concrete_rejections() {
    let h = Harness::new(json!({}));
    for (fixture, kind, outcome, status) in [
        (
            json!({"status":201,"body":"not json"}),
            "invalid_response",
            "created",
            Some(201),
        ),
        (
            json!({"status":201,"body":"{}"}),
            "invalid_response",
            "created",
            Some(201),
        ),
        (
            json!({"status":201,"body":r#"{"id":0,"issueKey":"P-1"}"#}),
            "invalid_response",
            "created",
            Some(201),
        ),
        (
            json!({"status":201,"body":"{}", "truncate":true}),
            "network",
            "created",
            Some(201),
        ),
        (json!({"disconnect":true}), "network", "unknown", None),
        (
            json!({"status":400,"body":r#"{"errors":[{"code":7,"message":"bad"}]}"#}),
            "api",
            "not_created",
            Some(400),
        ),
        (
            json!({"status":401,"body":r#"{"errors":[{"code":11,"message":"bad"}]}"#}),
            "auth",
            "not_created",
            Some(401),
        ),
        (
            json!({"status":403,"body":"{}"}),
            "permission",
            "unknown",
            Some(403),
        ),
        (
            json!({"status":429,"body":r#"{"errors":[{"code":13,"message":"busy"}]}"#}),
            "rate_limit",
            "not_created",
            Some(429),
        ),
        (
            json!({"status":400,"body":r#"{"errors":[{}]}"#}),
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
            json!({"status":200,"body":r#"{"id":1,"issueKey":"P-1"}"#}),
            "api",
            "unknown",
            Some(200),
        ),
    ] {
        std::fs::write(h.root.join("response.json"), fixture.to_string()).unwrap();
        failure(&h.run(&args()), 1, kind, outcome, status);
    }
    assert_eq!(
        h.requests().len(),
        14,
        "no redirect, retry or confirmation GET"
    );
}

#[test]
fn create_uses_defaults_names_self_dates_and_explicit_overrides_or_clear() {
    let h = Harness::new(json!({"routes":{
        "/api/v2/projects/P":{"body":"{\"id\":42}"},
        "/api/v2/projects/P/issueTypes":{"body":"[{\"id\":7,\"name\":\"タスク\"}]"},
        "/api/v2/priorities":{"body":"[{\"id\":3,\"name\":\"中\"}]"},
        "/api/v2/users/myself":{"body":"{\"id\":99}"},
        "/api/v2/projects/P/versions":{"body":"[{\"id\":8,\"name\":\"v1\"},{\"id\":8,\"name\":\"alias\"},{\"id\":9,\"name\":\"v2\"}]"},
        "/api/v2/issues":{"status":201,"body":"{\"id\":1,\"issueKey\":\"P-1\"}"}
    }}));
    std::fs::create_dir(h.root.join(".backlog")).unwrap();
    std::fs::write(h.root.join(".backlog/settings.json"), r#"{"project":"P","issue":{"create":{"issueType":"タスク","priority":"中","assignee":"self"},"list":{"status":["must not use"]}}}"#).unwrap();
    success(&h.run(&[
        "issue",
        "create",
        "-t",
        "test",
        "--start-date",
        "2024-02-29",
        "--due-date",
        "2023-01-01",
        "-m",
        "v1",
        "-m",
        "v1",
        "-m",
        "alias",
        "-m",
        "v2",
        "--json",
    ]));
    let r = h.requests();
    assert_eq!(r.len(), 6);
    assert_eq!(
        r[5]["form"],
        json!({"projectId":["42"],"summary":["test"],"issueTypeId":["7"],"priorityId":["3"],"assigneeId":["99"],"startDate":["2024-02-29"],"dueDate":["2023-01-01"],"milestoneId[]":["8","9"]})
    );
    for assignee in [
        vec!["--clear-assignee"],
        vec!["--assignee-id", "123456789012345678901234567890"],
    ] {
        let mut argv = args();
        argv.extend(assignee);
        success(&h.run(&argv));
    }
    let r = h.requests();
    assert_eq!(r.len(), 8);
    assert!(r[6]["form"].get("assigneeId").is_none());
    assert_eq!(
        r[7]["form"]["assigneeId"],
        json!(["123456789012345678901234567890"])
    );
}

#[test]
fn body_literal_file_and_explicit_stdin_preserve_empty_or_untrimmed_utf8() {
    use std::io::Write;
    use std::process::Stdio;
    let h = Harness::new(json!({"status":201,"body":"{\"id\":1,\"issueKey\":\"P-1\"}"}));
    let path = h.root.join("body.txt");
    for body in ["", " \n# **日本語** é 👩‍💻\t\r\n<&> test-secret\n "] {
        std::fs::write(&path, body).unwrap();
        let mut literal = args();
        literal.extend(["--body", body]);
        success(&h.run(&literal));
        let mut file = args();
        file.extend(["--body-file", path.to_str().unwrap()]);
        success(&h.run(&file));
        let mut stdin = args();
        stdin.extend(["--body-file", "-"]);
        let mut child = h
            .command(&stdin)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(body.as_bytes())
            .unwrap();
        success(&child.wait_with_output().unwrap());
        for r in h.requests().iter().rev().take(3) {
            assert_eq!(r["form"]["description"], json!([body]));
        }
    }
    std::fs::write(&path, [0xff]).unwrap();
    let mut argv = args();
    argv.extend(["--body-file", path.to_str().unwrap()]);
    failure(&h.run(&argv), 2, "input", "not_created", None);
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
    let mut argv = args();
    argv.extend(["--body-file", "missing"]);
    failure(&h.run(&argv), 2, "input", "not_created", None);
    assert_eq!(h.requests().len(), 6);
}

#[test]
fn create_input_failures_have_not_created_and_no_http_without_misidentifying_values() {
    let h = Harness::new(json!({"status":500}));
    for extra in [
        vec!["--body", "", "--body-file", "-"],
        vec!["--issue-type", "Task"],
        vec!["--priority", "中"],
        vec!["--milestone", "v1", "--milestone-id", "2"],
        vec!["--assignee", "self", "--clear-assignee"],
        vec!["--assignee-id", "2", "--clear-assignee"],
        vec!["--assignee", "self", "--assignee-id", "2"],
        vec!["--body", "a", "--body", "b"],
        vec!["--start-date", "2024-02-30"],
        vec!["--due-date", "1900-02-29"],
        vec!["--due-date", "2024-2-01"],
        vec!["--due-date", "2024-00-01"],
        vec!["--due-date", "2024-01-00"],
        vec!["--due-date", "0000-01-01"],
        vec!["--assignee-id", "0"],
        vec!["--milestone-id", "-1"],
        vec!["--unknown"],
    ] {
        let mut argv = args();
        argv.extend(extra);
        failure(&h.run(&argv), 2, "input", "not_created", None);
    }
    for argv in [
        vec!["issue", "create", "--json"],
        vec![
            "issue",
            "create",
            "-t",
            " ",
            "-p",
            "123",
            "--issue-type-id",
            "7",
            "--priority-id",
            "3",
            "--json",
        ],
        vec!["issue", "create", "-t", "x", "--json"],
        vec![
            "issue",
            "create",
            "-t",
            "x",
            "-p",
            "all",
            "--issue-type-id",
            "7",
            "--priority-id",
            "3",
            "--json",
        ],
        vec![
            "issue",
            "create",
            "-t",
            "x",
            "-p",
            "0",
            "--issue-type-id",
            "7",
            "--priority-id",
            "3",
            "--json",
        ],
        vec![
            "issue",
            "create",
            "-t",
            "x",
            "-p",
            "P",
            "--issue-type-id",
            "0",
            "--priority-id",
            "3",
            "--json",
        ],
        vec!["--json", "issue", "create", "-t", "x", "--json"],
    ] {
        failure(&h.run(&argv), 2, "input", "not_created", None);
    }
    for argv in [
        vec!["issue", "list", "issue", "create", "--json"],
        vec!["issue", "view", "issue", "create", "--json"],
        vec!["--profile", "issue", "create", "--json"],
    ] {
        common::failure(&h.run(&argv), 2, "input");
    }
    assert!(h.requests().is_empty());
    std::fs::write(h.root.join("backlog/config.json"), "broken").unwrap();
    failure(&h.run(&args()), 1, "config", "not_created", None);
    let help = h.run(&["issue", "create", "--help", "--json"]);
    assert!(help.status.success());
    assert!(help.stderr.is_empty());
    assert!(
        String::from_utf8(help.stdout)
            .unwrap()
            .contains("--body-file")
    );
}

#[test]
fn create_output_failures_and_redaction_collisions_preserve_certainty_in_both_modes() {
    use std::process::Stdio;
    let h = Harness::new(
        json!({"status":201,"body":r#"{"id":1,"issueKey":"P-1","test-secret":1,"[REDACTED]":2}"#}),
    );
    let error = failure(&h.run(&args()), 1, "output", "created", Some(201));
    assert!(!error.to_string().contains("test-secret"));
    std::fs::write(h.root.join("response.json"),json!({"status":400,"body":r#"{"errors":[{"code":7,"message":"test-secret\u001b","test-secret":1,"[REDACTED]":2}]}"#}).to_string()).unwrap();
    failure(&h.run(&args()), 1, "output", "not_created", Some(400));
    std::fs::write(h.root.join("response.json"),json!({"status":201,"body":r#"{"id":1,"issueKey":"P-1","test-secret":"test-secret\u001b"}"#}).to_string()).unwrap();
    let value = success(&h.run(&args()));
    assert_eq!(value["[REDACTED]"], "[REDACTED]\\u{1b}");
    let mut fixed = args();
    fixed.pop();
    let out = h.run(&fixed);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        format!("CREATED\tP-1\nURL\t{}/view/P-1\n", h.origin)
    );
    for argv in [args(), fixed] {
        let mut child = h
            .command(&argv)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        drop(child.stdout.take());
        let out = child.wait_with_output().unwrap();
        assert_eq!(out.status.code(), Some(1));
        if argv.contains(&"--json") {
            failure(&out, 1, "output", "created", Some(201));
        } else {
            assert_eq!(
                String::from_utf8(out.stderr).unwrap(),
                "error: Cannot write standard output [issue.create: created]\n"
            );
        }
    }
    std::fs::write(
        h.root.join("response.json"),
        json!({"disconnect":true}).to_string(),
    )
    .unwrap();
    let mut argv = args();
    argv.pop();
    let out = h.run(&argv);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        String::from_utf8(out.stderr)
            .unwrap()
            .contains("[issue.create: unknown]")
    );
}

#[test]
fn create_interrupts_before_send_after_send_and_after_201_keep_observed_certainty() {
    use std::process::{Command, Stdio};
    for (fixture, project, ready, outcome, status, method) in [
        (
            json!({"routes":{"/api/v2/projects/P":{"body":"{\"id\":42}","header_delay":32}}}),
            "P",
            "requests",
            "not_created",
            None,
            "GET",
        ),
        (
            json!({"header_delay":32}),
            "123",
            "requests",
            "unknown",
            None,
            "POST",
        ),
        (
            json!({"status":201,"body":"{}","delay":32}),
            "123",
            "headers-sent",
            "created",
            Some(201),
            "POST",
        ),
        (
            json!({"status":403,"body":r#"{"errors":[{"code":5,"message":"denied"}]}"#,"delay":32}),
            "123",
            "headers-sent",
            "unknown",
            Some(403),
            "POST",
        ),
    ] {
        let h = Harness::new(fixture);
        let mut argv = args();
        argv[3] = project;
        let child = h
            .command(&argv)
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
            std::thread::sleep(std::time::Duration::from_millis(500));
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
        assert_eq!(h.requests().len(), 1);
        assert_eq!(h.requests()[0]["method"], method);
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert_eq!(
            h.requests().len(),
            1,
            "no surviving send or retry after notification"
        );
    }
}

#[test]
fn create_body_timeout_retains_created_without_retry() {
    let h = Harness::new(json!({"status":201,"body":"{}","delay":32}));
    let start = std::time::Instant::now();
    failure(&h.run(&args()), 1, "network", "created", Some(201));
    assert!(start.elapsed() >= std::time::Duration::from_secs(29));
    assert!(start.elapsed() < std::time::Duration::from_secs(40));
    assert_eq!(h.requests().len(), 1);
}

#[test]
fn required_fields_repeated_single_flags_and_name_resolution_fail_without_post() {
    let h = Harness::new(json!({"body":"[]"}));
    for field in [
        "project",
        "issue-type-id",
        "priority-id",
        "title",
        "body-file",
        "assignee-id",
        "start-date",
        "due-date",
        "issue-type",
        "priority",
        "assignee",
        "clear-assignee",
    ] {
        let flag = format!("--{field}");
        let mut argv = vec![
            "issue",
            "create",
            "--json",
            "--title",
            "x",
            "--project",
            "123",
            "--issue-type-id",
            "7",
            "--priority-id",
            "3",
        ];
        if matches!(field, "project" | "title" | "issue-type-id" | "priority-id") {
            argv.extend([&flag, "1"]);
        } else if field == "clear-assignee" {
            argv.extend([flag.as_str(), flag.as_str()]);
        } else {
            argv.extend([&flag, "1", &flag, "1"]);
        }
        failure(&h.run(&argv), 2, "input", "not_created", None);
    }
    for argv in [
        vec![
            "issue",
            "create",
            "--json",
            "-t",
            "x",
            "--issue-type-id",
            "7",
            "--priority-id",
            "3",
        ],
        vec![
            "issue",
            "create",
            "--json",
            "-t",
            "x",
            "-p",
            "123",
            "--priority-id",
            "3",
        ],
        vec![
            "issue",
            "create",
            "--json",
            "-t",
            "x",
            "-p",
            "123",
            "--issue-type-id",
            "7",
        ],
    ] {
        failure(&h.run(&argv), 2, "input", "not_created", None);
    }
    assert!(h.requests().is_empty());
    for body in ["[]", r#"[{"id":1,"name":"Task"},{"id":2,"name":"Task"}]"#] {
        std::fs::write(
            h.root.join("response.json"),
            json!({"body":body}).to_string(),
        )
        .unwrap();
        failure(
            &h.run(&[
                "issue",
                "create",
                "--json",
                "-t",
                "x",
                "-p",
                "123",
                "--issue-type",
                "Task",
                "--priority-id",
                "3",
            ]),
            2,
            "input",
            "not_created",
            None,
        );
    }
    assert_eq!(h.requests().len(), 2);
    assert!(
        h.requests()
            .iter()
            .all(|r| r["method"] == "GET" && r["path"] == "/api/v2/projects/123/issueTypes")
    );
}

#[test]
fn create_stdin_is_only_read_explicitly_and_can_be_interrupted_before_any_request() {
    use std::process::{Command, Stdio};
    let h = Harness::new(json!({"status":201,"body":"{\"id\":1,\"issueKey\":\"P-1\"}"}));
    let mut child = h
        .command(&args())
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
            panic!("create read implicit stdin");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    success(&child.wait_with_output().unwrap());
    assert!(h.requests()[0]["form"].get("description").is_none());
    let mut argv = args();
    argv.extend(["--body-file", "-"]);
    let mut child = h
        .command(&argv)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let _open_stdin = child.stdin.take().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(500));
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
    assert_eq!(h.requests().len(), 1);
}

#[test]
fn create_global_profile_flags_and_tty_are_noninteractive() {
    let h = Harness::new(json!({"status":201,"body":"{\"id\":1,\"issueKey\":\"P-1\"}"}));
    h.config(json!({"profiles":{"work":{"spaceUrl":h.origin,"apiKeyEnv":"TEST_KEY"}}}));
    let mut argv = args();
    argv.splice(0..0, ["--profile", "work", "--no-input"]);
    success(&h.run(&argv));
    let mut argv = args();
    argv.extend(["--profile", "work", "--no-input"]);
    success(&h.run(&argv));
    let template = h.command(&[]);
    let mut argv = args();
    argv.pop();
    argv.extend(["--profile", "work", "--no-input"]);
    let out = std::process::Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/tty_cli.py"
        ))
        .arg(env!("CARGO_BIN_EXE_bl-cli"))
        .args(argv)
        .env_clear()
        .envs(template.get_envs().filter_map(|(k, v)| v.map(|v| (k, v))))
        .env("PATH", std::env::var_os("PATH").unwrap())
        .current_dir(&h.root)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(out.stderr.is_empty());
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        format!("CREATED\tP-1\r\nURL\t{}/view/P-1\r\n", h.origin)
    );
    assert_eq!(h.requests().len(), 3);
}
