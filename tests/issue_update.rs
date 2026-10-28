mod common;
use common::{Harness, success};
use serde_json::{Value, json};

fn latest() -> Value {
    json!({"projectId":42,"summary":"old","description":"old body","issueType":{"id":1},"priority":{"id":2},"status":{"id":3},"assignee":{"id":4},"resolution":null,"startDate":"2026-10-01T00:00:00Z","dueDate":null,"milestone":[{"id":5},{"id":6}]})
}
fn fixture(patch: Value) -> Value {
    json!({"routes":{
        "GET /api/v2/issues/P-1":{"body":latest().to_string()},
        "PATCH /api/v2/issues/P-1":patch
    }})
}
fn args() -> Vec<&'static str> {
    vec!["issue", "update", "P-1", "--json"]
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
        json!({"operation":"issue.update","outcome":outcome}),
        "{value}"
    );
    assert_eq!(value["error"]["httpStatus"], json!(status), "{value}");
    assert_eq!(output.stderr.last(), Some(&b'\n'));
    value
}
#[test]
fn update_sends_only_differing_fields_in_one_authenticated_patch() {
    let h = Harness::new(fixture(
        json!({"body":r#"{"id":123456789012345678901234567890,"issueKey":"P-1","unknown":{"raw":" 原文\n","huge":123456789012345678901234567890},"url":"https://evil.invalid"}"#}),
    ));
    std::fs::create_dir(h.root.join(".backlog")).unwrap();
    std::fs::write(h.root.join(".backlog/settings.json"),r#"{"project":"WRONG","issue":{"create":{"issueType":"unused","priority":"unused","assignee":"unused"}}}"#).unwrap();
    let mut argv = args();
    argv.extend([
        "-t",
        " 修正 ",
        "-b",
        " \n日本語 e\u{301}👩‍💻 &+= test-secret\u{1b}\t\n ",
        "--issue-type-id",
        "007",
        "--priority-id",
        "2",
        "--status-id",
        "8",
        "--clear-assignee",
        "--resolution-id",
        "000",
        "--start-date",
        "2026-10-01",
        "--due-date",
        "2000-02-29",
        "--milestone-id",
        "009",
        "--milestone-id",
        "9",
        "--milestone-id",
        "10",
    ]);
    let value = success(&h.run(&argv));
    assert_eq!(value["id"].to_string(), "123456789012345678901234567890");
    assert_eq!(
        value["unknown"]["huge"].to_string(),
        "123456789012345678901234567890"
    );
    assert_eq!(value["unknown"]["raw"], " 原文\n");
    assert_eq!(value["url"], format!("{}/view/P-1", h.origin));
    let r = h.requests();
    assert_eq!(r.len(), 2, "no candidate lookup, retry or confirmation GET");
    assert_eq!(r[0]["method"], "GET");
    assert_eq!(r[1]["method"], "PATCH");
    assert_eq!(r[1]["path"], "/api/v2/issues/P-1");
    assert_eq!(r[1]["headers"]["backlog-api-key"], "test-secret");
    assert!(
        r[1]["headers"]["content-type"]
            .as_str()
            .unwrap()
            .starts_with("application/x-www-form-urlencoded")
    );
    assert_eq!(
        r[1]["form"],
        json!({"summary":[" 修正 "],"description":[" \n日本語 e\u{301}👩‍💻 &+= test-secret\u{1b}\t\n "],"issueTypeId":["7"],"statusId":["8"],"assigneeId":[""],"resolutionId":["0"],"dueDate":["2000-02-29"],"milestoneId[]":["9","10"]})
    );
}

#[test]
fn update_noop_uses_calendar_dates_null_body_and_unordered_id_sets() {
    let mut response = fixture(json!({"body":"{\"id\":1,\"issueKey\":\"P-1\"}"}));
    let mut current = latest();
    current["description"] = Value::Null;
    response["routes"]["GET /api/v2/issues/P-1"]["body"] = json!(current.to_string());
    let h = Harness::new(response);
    for changes in [
        vec![],
        vec!["--body", ""],
        vec![
            "-t",
            "old",
            "--issue-type-id",
            "0001",
            "--priority-id",
            "2",
            "--status-id",
            "003",
            "--assignee-id",
            "4",
            "--clear-resolution",
            "--start-date",
            "2026-10-01",
            "--clear-due-date",
            "--milestone-id",
            "006",
            "--milestone-id",
            "5",
            "--milestone-id",
            "6",
        ],
    ] {
        let mut argv = args();
        argv.extend(changes);
        failure(&h.run(&argv), 2, "input", "not_updated", None);
    }
    assert_eq!(h.requests().len(), 3);
    assert!(h.requests().iter().all(|r| r["method"] == "GET"));
    current["description"] = json!("");
    current["resolution"] = json!({"id":0});
    std::fs::write(
        h.root.join("response.json"),
        json!({"body":current.to_string()}).to_string(),
    )
    .unwrap();
    failure(
        &h.run(&[
            "issue",
            "update",
            "P-1",
            "--body",
            "",
            "--resolution-id",
            "000",
            "--json",
        ]),
        2,
        "input",
        "not_updated",
        None,
    );
    assert_eq!(h.requests().len(), 4);
    assert_eq!(h.requests()[3]["method"], "GET");
}

#[test]
fn update_clears_use_empty_values_and_omit_already_empty_fields() {
    let mut response = fixture(json!({"body":"{\"id\":1,\"issueKey\":\"P-1\"}"}));
    let mut current = latest();
    current["resolution"] = json!({"id":0});
    current["dueDate"] = json!("2024-02-29T00:00:00Z");
    response["routes"]["GET /api/v2/issues/P-1"]["body"] = json!(current.to_string());
    let h = Harness::new(response);
    let mut argv = args();
    argv.extend([
        "-b",
        "",
        "--clear-assignee",
        "--clear-resolution",
        "--clear-start-date",
        "--clear-due-date",
        "--clear-milestones",
    ]);
    success(&h.run(&argv));
    assert_eq!(
        h.requests()[1]["form"],
        json!({"description":[""],"assigneeId":[""],"resolutionId":[""],"startDate":[""],"dueDate":[""],"milestoneId[]":[""]})
    );
    let mut current = latest();
    for field in [
        "description",
        "assignee",
        "resolution",
        "startDate",
        "dueDate",
    ] {
        current[field] = Value::Null;
    }
    current["milestone"] = json!([]);
    std::fs::write(
        h.root.join("response.json"),
        json!({"body":current.to_string()}).to_string(),
    )
    .unwrap();
    failure(&h.run(&argv), 2, "input", "not_updated", None);
    assert_eq!(h.requests().len(), 3);
}

#[test]
fn update_names_use_target_project_self_and_canonical_milestone_ids() {
    let mut response = fixture(json!({"body":"{\"id\":1,\"issueKey\":\"P-1\"}"}));
    for (path, body) in [
        (
            "/api/v2/projects/42/issueTypes",
            r#"[{"id":7,"name":"Task"}]"#,
        ),
        ("/api/v2/priorities", r#"[{"id":8,"name":"中"}]"#),
        (
            "/api/v2/projects/42/statuses",
            r#"[{"id":9,"name":"完了"}]"#,
        ),
        ("/api/v2/projects/42/users", r#"[{"id":10,"name":"担当"}]"#),
        ("/api/v2/resolutions", r#"[{"id":0,"name":"対応済み"}]"#),
        (
            "/api/v2/projects/42/versions",
            r#"[{"id":11,"name":"v1"},{"id":11,"name":"alias"},{"id":12,"name":"v2"}]"#,
        ),
        ("/api/v2/users/myself", r#"{"id":13}"#),
    ] {
        response["routes"][path] = json!({"body":body});
    }
    let h = Harness::new(response);
    std::fs::create_dir(h.root.join(".backlog")).unwrap();
    std::fs::write(h.root.join(".backlog/settings.json"),r#"{"project":"WRONG","issue":{"create":{"issueType":"unused","priority":"unused","assignee":"unused"}}}"#).unwrap();
    let mut argv = args();
    argv.extend([
        "--issue-type",
        "Task",
        "--priority",
        "中",
        "-s",
        "完了",
        "-a",
        "担当",
        "--resolution",
        "対応済み",
        "-m",
        "v1",
        "-m",
        "v1",
        "-m",
        "alias",
        "-m",
        "v2",
    ]);
    success(&h.run(&argv));
    let r = h.requests();
    assert_eq!(r.len(), 8);
    assert_eq!(
        r[7]["form"],
        json!({"issueTypeId":["7"],"priorityId":["8"],"statusId":["9"],"assigneeId":["10"],"resolutionId":["0"],"milestoneId[]":["11","12"]})
    );
    let mut argv = args();
    argv.extend(["-a", "self"]);
    success(&h.run(&argv));
    let r = h.requests();
    assert_eq!(r.len(), 11);
    assert_eq!(r[9]["path"], "/api/v2/users/myself");
    assert_eq!(r[10]["form"], json!({"assigneeId":["13"]}));
}

#[test]
fn update_validates_flags_ids_title_dates_and_config_before_requests() {
    let h = Harness::new(fixture(json!({})));
    for changes in [
        vec!["-t", " \n\t"],
        vec!["--start-date", "2100-02-29"],
        vec!["--due-date", "2024-04-31"],
        vec!["--due-date", "0000-01-01"],
        vec!["--start-date", "2024-2-01"],
        vec!["--issue-type-id", "0"],
        vec!["--priority-id", "-1"],
        vec!["--status-id", "1.2"],
        vec!["--assignee-id", "1e2"],
        vec!["--milestone-id", "0"],
        vec!["--resolution-id", "-1"],
        vec!["--issue-type", "Task", "--issue-type-id", "1"],
        vec!["--priority", "中", "--priority-id", "1"],
        vec!["-s", "open", "--status-id", "1"],
        vec!["--milestone-id", "1", "-m", "v1"],
        vec!["-a", "self", "--assignee-id", "1"],
        vec!["--resolution", "done", "--resolution-id", "0"],
        vec!["--clear-assignee", "-a", "self"],
        vec!["--clear-assignee", "--assignee-id", "1"],
        vec!["--clear-resolution", "--resolution-id", "0"],
        vec!["--clear-resolution", "--resolution", "done"],
        vec!["--clear-start-date", "--start-date", "2024-01-01"],
        vec!["--clear-due-date", "--due-date", "2024-01-01"],
        vec!["--clear-milestones", "-m", "v1"],
        vec!["--clear-milestones", "--milestone-id", "1"],
        vec!["--project", "42"],
        vec!["--created", "2024-01-01"],
        vec!["--updated", "2024-01-01"],
        vec!["--body", "", "--body-file", ""],
        vec!["--comment", "not supported"],
    ] {
        let mut argv = args();
        argv.extend(changes);
        failure(&h.run(&argv), 2, "input", "not_updated", None);
    }
    for flag in [
        "--title",
        "--body",
        "--body-file",
        "--issue-type",
        "--issue-type-id",
        "--priority",
        "--priority-id",
        "--status",
        "--status-id",
        "--assignee",
        "--assignee-id",
        "--resolution",
        "--resolution-id",
        "--start-date",
        "--due-date",
    ] {
        let mut argv = args();
        argv.extend([flag, "1", flag, "2"]);
        failure(&h.run(&argv), 2, "input", "not_updated", None);
    }
    for flag in [
        "--clear-assignee",
        "--clear-resolution",
        "--clear-start-date",
        "--clear-due-date",
        "--clear-milestones",
        "--json",
        "--no-input",
    ] {
        let mut argv = args();
        argv.extend([flag, flag]);
        failure(&h.run(&argv), 2, "input", "not_updated", None);
    }
    for issue in ["", "0", "P-0", "../P-1"] {
        failure(
            &h.run(&["issue", "update", issue, "--json", "-t", "new"]),
            2,
            "input",
            "not_updated",
            None,
        );
    }
    failure(
        &h.run(&["issue", "update", "--json"]),
        2,
        "input",
        "not_updated",
        None,
    );
    for argv in [
        vec!["issue", "list", "issue", "update", "--json"],
        vec!["issue", "view", "issue", "update", "--json"],
        vec!["--profile", "issue", "update", "--json"],
    ] {
        common::failure(&h.run(&argv), 2, "input");
    }
    assert!(h.requests().is_empty());
    std::fs::write(h.root.join("backlog/config.json"), "broken").unwrap();
    failure(&h.run(&args()), 1, "config", "not_updated", None);
    for flag in ["--help", "-h"] {
        let out = h.run(&["issue", "update", flag, "--json"]);
        assert_eq!(out.status.code(), Some(0));
        assert!(out.stderr.is_empty());
        let help = String::from_utf8(out.stdout).unwrap();
        for expected in [
            "<ISSUE_ID_OR_KEY>",
            "--body-file",
            "--clear-milestones",
            "--resolution-id",
            "--status-id",
        ] {
            assert!(help.contains(expected), "{help}");
        }
        assert!(!help.contains("--project"));
    }
}

#[test]
fn update_literal_file_and_explicit_stdin_preserve_utf8_empty_and_omitted_body() {
    use std::io::Write;
    use std::process::Stdio;
    let h = Harness::new(fixture(json!({"body":"{\"id\":1,\"issueKey\":\"P-1\"}"})));
    let raw = " \n# **日本語** e\u{301}👩‍💻 test-secret\u{1b}\t\n ";
    for content in [raw, ""] {
        std::fs::write(h.root.join("body.txt"), content).unwrap();
        for source in [
            vec!["--body", content],
            vec!["--body-file", "body.txt"],
            vec!["--body-file", "-"],
        ] {
            let mut argv = args();
            argv.extend(source.clone());
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
                    .write_all(content.as_bytes())
                    .unwrap();
            } else {
                drop(child.stdin.take());
            }
            success(&child.wait_with_output().unwrap());
            assert_eq!(
                h.requests().last().unwrap()["form"],
                json!({"description":[content]})
            );
        }
    }
    std::fs::write(h.root.join("bad.txt"), [0xff, 0xfe]).unwrap();
    for path in ["bad.txt", "missing.txt"] {
        let mut argv = args();
        argv.extend(["--body-file", path]);
        failure(&h.run(&argv), 2, "input", "not_updated", None);
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
        "not_updated",
        None,
    );
    assert_eq!(
        h.requests().len(),
        12,
        "invalid body never fetches or patches"
    );
    let mut argv = args();
    argv.extend(["-t", "new"]);
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
            panic!("update read implicit stdin");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    success(&child.wait_with_output().unwrap());
    assert_eq!(
        h.requests().last().unwrap()["form"],
        json!({"summary":["new"]})
    );
}

#[test]
fn update_preflight_and_name_errors_never_patch() {
    let h = Harness::new(json!({}));
    for (response, kind, status) in [
        (json!({"disconnect":true}), "network", None),
        (json!({"status":403}), "permission", Some(403)),
        (json!({"body":"not json"}), "invalid_response", Some(200)),
        (json!({"body":"{}"}), "invalid_response", Some(200)),
        (
            json!({"body":"{\"projectId\":0,\"summary\":\"old\"}"}),
            "invalid_response",
            Some(200),
        ),
    ] {
        std::fs::write(h.root.join("response.json"), response.to_string()).unwrap();
        let mut argv = args();
        argv.extend(["-t", "new"]);
        failure(&h.run(&argv), 1, kind, "not_updated", status);
    }
    for (body, kind, status) in [
        ("[]", "input", None),
        (
            r#"[{"id":1,"name":"Task"},{"id":2,"name":"Task"}]"#,
            "input",
            None,
        ),
        (r#"[{"name":"Task"}]"#, "invalid_response", Some(200)),
    ] {
        let mut response = fixture(json!({}));
        response["routes"]["/api/v2/projects/42/issueTypes"] = json!({"body":body});
        std::fs::write(h.root.join("response.json"), response.to_string()).unwrap();
        let mut argv = args();
        argv.extend(["--issue-type", "Task"]);
        failure(
            &h.run(&argv),
            if kind == "input" { 2 } else { 1 },
            kind,
            "not_updated",
            status,
        );
    }
    assert!(h.requests().iter().all(|r| r["method"] == "GET"));
}

#[test]
fn update_certainty_trusts_concrete_rejections_and_keeps_updated_after_200() {
    let h = Harness::new(json!({}));
    for (patch, kind, outcome, status) in [
        (
            json!({"body":"not json"}),
            "invalid_response",
            "updated",
            Some(200),
        ),
        (
            json!({"body":"{}"}),
            "invalid_response",
            "updated",
            Some(200),
        ),
        (
            json!({"body":"{\"id\":0,\"issueKey\":\"P-1\"}"}),
            "invalid_response",
            "updated",
            Some(200),
        ),
        (
            json!({"body":"{\"id\":1,\"issueKey\":\"../P-1\"}"}),
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
            json!({"status":201,"body":"{\"id\":1,\"issueKey\":\"P-1\"}"}),
            "api",
            "unknown",
            Some(201),
        ),
    ] {
        std::fs::write(h.root.join("response.json"), fixture(patch).to_string()).unwrap();
        let mut argv = args();
        argv.extend(["-t", "new"]);
        let error = failure(&h.run(&argv), 1, kind, outcome, status);
        assert!(!error.to_string().contains("test-secret"));
    }
    let r = h.requests();
    assert_eq!(
        r.len(),
        34,
        "one GET and one PATCH each, no retry, redirect or confirmation GET"
    );
    assert_eq!(r.iter().filter(|r| r["method"] == "PATCH").count(), 17);
}

#[test]
fn update_output_is_safe_and_output_failures_retain_updated_in_both_modes() {
    use std::process::Stdio;
    let h = Harness::new(fixture(
        json!({"body":r#"{"id":1,"issueKey":"P-1","test-secret":1,"[REDACTED]":2}"#}),
    ));
    let mut argv = args();
    argv.extend(["-t", "new"]);
    failure(&h.run(&argv), 1, "output", "updated", Some(200));
    std::fs::write(h.root.join("response.json"),fixture(json!({"status":400,"body":r#"{"errors":[{"code":7,"message":"test-secret\u001b","test-secret":1,"[REDACTED]":2}]}"#})).to_string()).unwrap();
    failure(&h.run(&argv), 1, "output", "not_updated", Some(400));
    std::fs::write(h.root.join("response.json"),fixture(json!({"body":r#"{"id":1,"issueKey":"P-1","test-secret":"test-secret\u001b","raw":" 原文\n"}"#})).to_string()).unwrap();
    let value = success(&h.run(&argv));
    assert_eq!(value["[REDACTED]"], "[REDACTED]\\u{1b}");
    assert_eq!(value["raw"], " 原文\n");
    let fixed = vec!["issue", "update", "P-1", "-t", "new"];
    let out = h.run(&fixed);
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stderr.is_empty());
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        format!("UPDATED\tP-1\nURL\t{}/view/P-1\n", h.origin)
    );
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
            failure(&out, 1, "output", "updated", Some(200));
        } else {
            assert_eq!(out.status.code(), Some(1));
            assert_eq!(
                String::from_utf8(out.stderr).unwrap(),
                "error: Cannot write standard output [issue.update: updated]\n"
            );
        }
    }
    std::fs::write(
        h.root.join("response.json"),
        fixture(json!({"disconnect":true})).to_string(),
    )
    .unwrap();
    let out = h.run(&["issue", "update", "P-1", "-t", "new"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        String::from_utf8(out.stderr)
            .unwrap()
            .contains("[issue.update: unknown]")
    );
}

#[test]
fn update_interrupts_keep_certainty_at_preflight_send_and_200_headers() {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    for (preflight, patch, ready, requests, outcome, status) in [
        (
            json!({"header_delay":32}),
            json!({}),
            "requests",
            1,
            "not_updated",
            None,
        ),
        (
            json!({"delay":32,"body":latest().to_string()}),
            json!({}),
            "get-headers-sent",
            1,
            "not_updated",
            Some(200),
        ),
        (
            json!({"body":latest().to_string()}),
            json!({"header_delay":32}),
            "requests",
            2,
            "unknown",
            None,
        ),
        (
            json!({"body":latest().to_string()}),
            json!({"body":"{}","delay":32}),
            "patch-headers-sent",
            2,
            "updated",
            Some(200),
        ),
        (
            json!({"body":latest().to_string()}),
            json!({"status":403,"body":r#"{"errors":[{"code":5,"message":"denied"}]}"#,"delay":32}),
            "patch-headers-sent",
            2,
            "unknown",
            Some(403),
        ),
    ] {
        let mut response = fixture(patch);
        response["routes"]["GET /api/v2/issues/P-1"] = preflight;
        let h = Harness::new(response);
        let mut argv = args();
        argv.extend(["-t", "new"]);
        let child = h
            .command(&argv)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let start = Instant::now();
        while !h.root.join(ready).exists() || h.requests().len() != requests {
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
        assert_eq!(h.requests().len(), requests);
        assert_eq!(
            h.requests().last().unwrap()["method"],
            if requests == 1 { "GET" } else { "PATCH" }
        );
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(h.requests().len(), requests, "no surviving send or retry");
    }
}

#[test]
fn update_explicit_stdin_wait_can_be_interrupted_before_preflight() {
    use std::process::{Command, Stdio};
    let h = Harness::new(fixture(json!({})));
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
fn update_body_timeout_keeps_updated_without_retry() {
    let h = Harness::new(fixture(json!({"body":"{}","delay":32})));
    let mut argv = args();
    argv.extend(["-t", "new"]);
    let start = std::time::Instant::now();
    failure(&h.run(&argv), 1, "network", "updated", Some(200));
    assert!(start.elapsed() >= std::time::Duration::from_secs(29));
    assert!(start.elapsed() < std::time::Duration::from_secs(40));
    assert_eq!(h.requests().len(), 2);
    assert_eq!(h.requests()[1]["method"], "PATCH");
}

#[test]
fn update_global_profile_flags_and_tty_are_noninteractive() {
    let h = Harness::new(fixture(json!({"body":"{\"id\":1,\"issueKey\":\"P-1\"}"})));
    h.config(json!({"profiles":{"work":{"spaceUrl":h.origin,"apiKeyEnv":"TEST_KEY"}}}));
    let mut argv = args();
    argv.extend(["-t", "new"]);
    argv.splice(0..0, ["--profile", "work", "--no-input"]);
    success(&h.run(&argv));
    let mut argv = args();
    argv.extend(["-t", "new", "--profile", "work", "--no-input"]);
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
            "update",
            "P-1",
            "-t",
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
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(out.stderr.is_empty());
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        format!("UPDATED\tP-1\r\nURL\t{}/view/P-1\r\n", h.origin)
    );
    assert_eq!(h.requests().len(), 6);
}

#[test]
fn update_connection_and_tls_failures_are_not_updated_without_http_writes() {
    use std::net::{TcpListener, TcpStream};
    use std::time::Duration;
    let h = Harness::new(fixture(json!({})));
    let mut argv = args();
    argv.extend(["-t", "new"]);
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
    assert!(h.requests().is_empty());
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    assert_eq!(
        TcpStream::connect_timeout(&address, Duration::from_secs(3))
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::ConnectionRefused
    );
    h.config(json!({"profiles":{"default":{"spaceUrl":format!("https://{address}"),"apiKeyEnv":"TEST_KEY"}}}));
    failure(&h.run(&argv), 1, "network", "not_updated", None);
    assert!(h.requests().is_empty());
}
