mod common;
use common::{Harness, failure, success};
use serde_json::{Value, json};

fn comment(id: u64) -> Value {
    json!({"id":id,"content":"# **日本語** é 👩‍💻\n本文","createdUser":{"name":"投稿者"},"created":"2026-10-01T01:02:03Z","updated":"2026-10-02T04:05:06Z","changeLog":[{"field":"limitDate","originalValue":"2026-10-01","newValue":"2026-11-01"}],"unknown":{"large":serde_json::from_str::<Value>("123456789012345678901234567890").unwrap(),"raw":"  原文\n\t<&>  "}})
}
fn fixture() -> Value {
    json!({"routes":{
        "/api/v2/issues/P-1/comments":{"commentItems":[comment(1),comment(2),comment(3),comment(4),comment(5)]},
        "/api/v2/issues/P-1/comments/3":{"body":comment(3).to_string()},
        "/api/v2/issues/P-1":{"body":"{\"projectId\":987654321098765432109876543210}"},
        "/api/v2/projects/987654321098765432109876543210":{"body":"{\"textFormattingRule\":\"markdown\"}"}
    }})
}
#[test]
fn list_fetches_one_descending_page_with_exclusive_id_boundaries_and_no_extra_gets() {
    let h = Harness::new(fixture());
    for (flags, ids, query) in [
        (vec![], vec![5, 4, 3, 2, 1], "count=20&order=desc"),
        (vec!["--limit", "2"], vec![5, 4], "count=2&order=desc"),
        (
            vec!["--min-id", "3"],
            vec![5, 4],
            "count=20&order=desc&minId=3",
        ),
        (
            vec!["--max-id", "3"],
            vec![2, 1],
            "count=20&order=desc&maxId=3",
        ),
        (
            vec!["--min-id", "2", "--max-id", "5"],
            vec![4, 3],
            "count=20&order=desc&minId=2&maxId=5",
        ),
        (vec!["--min-id", "5"], vec![], "count=20&order=desc&minId=5"),
    ] {
        let mut args = vec!["issue", "comment", "list", "P-1", "--json"];
        args.extend(flags);
        let value = success(&h.run(&args));
        assert_eq!(
            value,
            json!({"items":ids.into_iter().map(comment).collect::<Vec<_>>() })
        );
        let requests = h.requests();
        let request = requests.last().unwrap();
        assert_eq!(
            request["path"],
            format!("/api/v2/issues/P-1/comments?{query}")
        );
        assert_eq!(request["method"], "GET");
        assert_eq!(request["headers"]["backlog-api-key"], "test-secret");
    }
    assert_eq!(h.requests().len(), 6);
}

#[test]
fn view_keeps_body_and_history_and_unknown_fields_without_issue_or_project_gets() {
    let h = Harness::new(fixture());
    assert_eq!(
        success(&h.run(&["issue", "comment", "view", "P-1", "3", "--json"])),
        comment(3)
    );
    let requests = h.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0]["path"], "/api/v2/issues/P-1/comments/3");
}

#[test]
fn fixed_comments_show_body_and_history_together_and_null_content_history_is_not_blank() {
    let mut both = comment(3);
    both["changeLog"] = json!([
        {"field":"limitDate","originalValue":"2026-10-01","newValue":"2026-11-01"},
        {"field":"assigner","originalValue":null,"newValue":"担当 test-secret\u{1b}[31m"},
        {"field":"future-test-secret\u{1b}","originalValue":{"nested-test-secret":[12345678901234567890u64]},"newValue":["未来\u{7}",true]}
    ]);
    let mut history = comment(2);
    history["content"] = Value::Null;
    let mut response = fixture();
    response["routes"]["/api/v2/issues/P-1/comments"]["commentItems"] =
        json!([history.clone(), both.clone()]);
    response["routes"]["/api/v2/issues/P-1/comments/3"]["body"] = json!(both.to_string());
    let h = Harness::new(response);
    std::fs::create_dir(h.root.join(".backlog")).unwrap();
    std::fs::write(
        h.root.join(".backlog/settings.json"),
        r#"{"project":"IGNORE"}"#,
    )
    .unwrap();
    for command in [
        vec!["issue", "comment", "list", "P-1"],
        vec!["issue", "comment", "view", "P-1", "3"],
    ] {
        let output = h.run(&command);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert!(output.stderr.is_empty());
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains("ID\t3\nTYPE\tCOMMENT\nAUTHOR\t投稿者\nCREATED\t2026-10-01T01:02:03Z\nUPDATED\t2026-10-02T04:05:06Z\nBODY\n日本語 é 👩‍💻\n本文\nCHANGES\n"),"{text}");
        assert!(
            text.contains("DUE DATE\t2026-10-01 -> 2026-11-01\n"),
            "{text}"
        );
        assert!(
            text.contains("ASSIGNEE\tnull -> 担当 [REDACTED]\\u{1b}[31m\n"),
            "{text}"
        );
        assert!(text.contains("future-[REDACTED]\\u{1b}\t{\"nested-[REDACTED]\":[12345678901234567890]} -> [\"未来\\\\u{7}\",true]\n"),"{text}");
        assert!(!text.contains("test-secret"));
        assert!(!text.contains('\u{1b}'));
        assert!(!text.contains('\u{7}'));
        if command[2] == "list" {
            assert!(text.contains("ID\t2\nTYPE\tCHANGE HISTORY\n"), "{text}");
            assert_eq!(text.matches("DUE DATE\t").count(), 2);
        }
    }
    let requests = h.requests();
    assert_eq!(requests.len(), 6);
    for request in [&requests[1], &requests[4]] {
        assert_eq!(request["path"], "/api/v2/issues/P-1");
    }
    for request in [&requests[2], &requests[5]] {
        assert_eq!(
            request["path"],
            "/api/v2/projects/987654321098765432109876543210"
        );
    }
}

#[test]
fn null_history_only_list_and_view_succeed_without_issue_or_project_access() {
    let mut history = comment(3);
    history["content"] = Value::Null;
    history["changeLog"] = json!([
        {"field":"limitDate","originalValue":"2026-10-01","newValue":"2026-11-01"},
        {"field":"future-test-secret\u{1b}","originalValue":"前 test-secret\u{7}","newValue":{"nested-test-secret":["後\u{1b}",true]}}
    ]);
    let mut older = history.clone();
    older["id"] = json!(2);
    let mut response = fixture();
    response["routes"]["/api/v2/issues/P-1/comments"]["commentItems"] =
        json!([older, history.clone()]);
    response["routes"]["/api/v2/issues/P-1/comments/3"]["body"] = json!(history.to_string());
    for endpoint in [
        "/api/v2/issues/P-1",
        "/api/v2/projects/987654321098765432109876543210",
    ] {
        response["routes"][endpoint] = json!({"status":403});
    }
    let h = Harness::new(response);
    let outputs = [
        h.run(&["issue", "comment", "list", "P-1"]),
        h.run(&["issue", "comment", "view", "P-1", "3"]),
    ];
    assert_eq!(
        outputs.each_ref().map(|output| output.status.code()),
        [Some(0), Some(0)],
        "{outputs:?}"
    );
    for output in outputs {
        assert!(output.stderr.is_empty());
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains("ID\t3\nTYPE\tCHANGE HISTORY\n"), "{text}");
        assert!(
            text.contains("DUE DATE\t2026-10-01 -> 2026-11-01\n"),
            "{text}"
        );
        assert!(text.contains("future-[REDACTED]\\u{1b}\t前 [REDACTED]\\u{7} -> {\"nested-[REDACTED]\":[\"後\\\\u{1b}\",true]}\n"), "{text}");
        assert!(!text.contains("BODY\n"), "{text}");
        assert!(!text.contains("test-secret"));
        assert!(!text.contains('\u{1b}'));
        assert!(!text.contains('\u{7}'));
    }
    let requests = h.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0]["path"],
        "/api/v2/issues/P-1/comments?count=20&order=desc"
    );
    assert_eq!(requests[1]["path"], "/api/v2/issues/P-1/comments/3");
    assert!(requests.iter().all(|request| request["method"] == "GET"));
}

#[test]
fn invalid_limits_identifiers_boundaries_and_order_never_reach_https() {
    let h = Harness::new(fixture());
    for flags in [
        vec!["--limit", "0"],
        vec!["--limit", "101"],
        vec!["--limit", "-1"],
        vec!["--limit", "1.5"],
        vec!["--limit", "256"],
        vec!["--min-id", "0"],
        vec!["--max-id", "000"],
        vec!["--min-id", "-1"],
        vec!["--max-id", "1e2"],
        vec!["--min-id", "2", "--max-id", "2"],
        vec!["--min-id", "0002", "--max-id", "2"],
        vec!["--min-id", "3", "--max-id", "2"],
        vec![
            "--min-id",
            "123456789012345678901234567891",
            "--max-id",
            "123456789012345678901234567890",
        ],
        vec!["--limit", "2", "--limit", "3"],
        vec!["--min-id", "2", "--min-id", "3"],
        vec!["--max-id", "2", "--max-id", "3"],
        vec!["--offset", "0"],
        vec!["--order", "asc"],
        vec!["--project", "OTHER"],
    ] {
        let mut args = vec!["issue", "comment", "list", "P-1", "--json"];
        args.extend(flags);
        failure(&h.run(&args), 2, "input");
    }
    for id in [
        "",
        "0",
        "000",
        "-1",
        "1.5",
        "../P-1",
        "P-0",
        "P-1?x",
        "https://evil.test/P-1",
    ] {
        failure(
            &h.run(&["issue", "comment", "list", "--json", "--", id]),
            2,
            "input",
        );
        failure(
            &h.run(&["issue", "comment", "view", "--json", "--", id, "3"]),
            2,
            "input",
        );
        failure(
            &h.run(&["issue", "comment", "view", "--json", "--", "P-1", id]),
            2,
            "input",
        );
    }
    assert!(h.requests().is_empty());
}

#[test]
fn empty_page_is_success_in_both_modes_and_api_failures_are_not_empty_pages() {
    let h = Harness::new(fixture());
    assert_eq!(
        success(&h.run(&["issue", "comment", "list", "P-1", "--min-id", "5", "--json"])),
        json!({"items":[]})
    );
    let output = h.run(&["issue", "comment", "list", "P-1", "--min-id", "5"]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"No comments\n");
    assert!(output.stderr.is_empty());
    assert_eq!(h.requests().len(), 2);
    for (status, kind) in [
        (401, "auth"),
        (403, "permission"),
        (404, "not_found"),
        (429, "rate_limit"),
        (500, "api"),
        (302, "api"),
    ] {
        std::fs::write(h.root.join("response.json"), json!({"status":status,"location":"https://evil.invalid/","body":"{\"errors\":[{\"code\":5,\"message\":\"test-secret\\u001b[31m\"}]}"}).to_string()).unwrap();
        for args in [
            vec!["issue", "comment", "list", "P-1"],
            vec!["issue", "comment", "view", "P-1", "3"],
        ] {
            let mut json_args = args.clone();
            json_args.push("--json");
            let error = failure(&h.run(&json_args), 1, kind);
            assert_eq!(error["error"]["httpStatus"], status);
            assert_eq!(error["error"]["apiErrors"][0]["code"], 5);
            assert!(!error.to_string().contains("test-secret"));
            let output = h.run(&args);
            assert_eq!(output.status.code(), Some(1));
            assert!(output.stdout.is_empty());
            assert!(
                !String::from_utf8(output.stderr)
                    .unwrap()
                    .contains("test-secret")
            );
        }
    }
    assert_eq!(
        h.requests().len(),
        26,
        "no redirect, retry, count or extra pages"
    );
}

#[test]
fn invalid_required_comment_fields_and_malformed_responses_fail_atomically() {
    let h = Harness::new(fixture());
    let mut bad_items = vec![Value::Null, json!([]), json!(1), json!({})];
    for field in [
        "id",
        "content",
        "createdUser",
        "created",
        "updated",
        "changeLog",
    ] {
        let mut missing = comment(3);
        missing.as_object_mut().unwrap().remove(field);
        bad_items.push(missing);
        let mut bad = comment(3);
        bad[field] = json!(true);
        bad_items.push(bad);
    }
    for (field, bad) in [
        ("id", json!(0)),
        ("id", json!(1.5)),
        ("createdUser", json!({"name":1})),
        ("changeLog", json!([{}])),
        (
            "changeLog",
            json!([{"field":true,"originalValue":null,"newValue":null}]),
        ),
        ("changeLog", json!([{"field":"status","newValue":"完了"}])),
    ] {
        let mut value = comment(3);
        value[field] = bad;
        bad_items.push(value);
    }
    for bad in bad_items {
        for (args, body) in [
            (
                vec!["issue", "comment", "list", "P-1", "--json"],
                json!([comment(1), bad.clone()]).to_string(),
            ),
            (
                vec!["issue", "comment", "view", "P-1", "3", "--json"],
                bad.to_string(),
            ),
        ] {
            std::fs::write(
                h.root.join("response.json"),
                json!({"body":body}).to_string(),
            )
            .unwrap();
            let error = failure(&h.run(&args), 1, "invalid_response");
            assert_eq!(error["error"]["httpStatus"], 200);
        }
    }
    for body in ["null", "{}", "1", "not json"] {
        std::fs::write(
            h.root.join("response.json"),
            json!({"body":body}).to_string(),
        )
        .unwrap();
        failure(
            &h.run(&["issue", "comment", "list", "P-1", "--json"]),
            1,
            "invalid_response",
        );
    }
    std::fs::write(
        h.root.join("response.json"),
        json!({"disconnect":true}).to_string(),
    )
    .unwrap();
    for args in [
        vec!["issue", "comment", "list", "P-1", "--json"],
        vec!["issue", "comment", "view", "P-1", "3", "--json"],
    ] {
        let error = failure(&h.run(&args), 1, "network");
        assert!(error["error"]["httpStatus"].is_null());
    }
    assert!(
        h.requests()
            .iter()
            .all(|r| r["path"].as_str().unwrap().contains("/comments"))
    );
}

#[test]
fn numeric_ids_and_boundaries_keep_arbitrary_precision_without_membership_checks() {
    let mut original = comment(3);
    original["id"] = serde_json::from_str("123456789012345678901234567890").unwrap();
    let h = Harness::new(json!({"body":original.to_string()}));
    for (issue, id) in [
        ("0007", "0003"),
        (
            "123456789012345678901234567890",
            "123456789012345678901234567891",
        ),
    ] {
        assert_eq!(
            success(&h.run(&["issue", "comment", "view", issue, id, "--json"])),
            original
        );
    }
    std::fs::write(
        h.root.join("response.json"),
        json!({"body":"[]"}).to_string(),
    )
    .unwrap();
    for limit in ["1", "100"] {
        success(&h.run(&[
            "issue",
            "comment",
            "list",
            "0007",
            "--limit",
            limit,
            "--min-id",
            "000123456789012345678901234567890",
            "--max-id",
            "123456789012345678901234567891",
            "--json",
        ]));
    }
    let requests = h.requests();
    assert_eq!(requests.len(), 4);
    assert_eq!(requests[0]["path"], "/api/v2/issues/0007/comments/0003");
    assert_eq!(
        requests[1]["path"],
        "/api/v2/issues/123456789012345678901234567890/comments/123456789012345678901234567891"
    );
    assert_eq!(
        requests[2]["path"],
        "/api/v2/issues/0007/comments?count=1&order=desc&minId=000123456789012345678901234567890&maxId=123456789012345678901234567891"
    );
}

#[test]
fn json_preserves_null_history_and_raw_body_but_safely_redacts_all_external_values() {
    let mut original = comment(3);
    original["content"] = json!("# 原文\n[[P-2]] test-secret\u{1b}]52;c;bad\u{7}");
    original["changeLog"] = json!([{ "field":"future-test-secret", "originalValue":["test-secret\u{9b}"], "newValue":{"test-secret":"未来"},"extra":12345678901234567890u64 }]);
    original["test-secret-field"] = json!(["test-secret",{"nested-test-secret":true}]);
    let h = Harness::new(json!({"body":original.to_string()}));
    let output = h.run(&["issue", "comment", "view", "P-1", "3", "--json"]);
    let result = success(&output);
    assert_eq!(
        result["content"],
        "# 原文\n[[P-2]] [REDACTED]\\u{1b}]52;c;bad\\u{7}"
    );
    assert_eq!(result["changeLog"][0]["field"], "future-[REDACTED]");
    assert_eq!(
        result["changeLog"][0]["originalValue"],
        json!(["[REDACTED]\\u{9b}"])
    );
    assert_eq!(
        result["changeLog"][0]["newValue"],
        json!({"[REDACTED]":"未来"})
    );
    assert_eq!(
        result["[REDACTED]-field"],
        json!(["[REDACTED]",{"nested-[REDACTED]":true}])
    );
    assert!(
        !String::from_utf8(output.stdout)
            .unwrap()
            .contains("test-secret")
    );
    original["[REDACTED]-field"] = json!("collision");
    std::fs::write(
        h.root.join("response.json"),
        json!({"body":original.to_string()}).to_string(),
    )
    .unwrap();
    let error = failure(
        &h.run(&["issue", "comment", "view", "P-1", "3", "--json"]),
        1,
        "output",
    );
    assert_eq!(error["error"]["httpStatus"], 200);
    let mut history = comment(2);
    history["content"] = Value::Null;
    let mut body_only = comment(1);
    body_only["changeLog"] = Value::Null;
    std::fs::write(
        h.root.join("response.json"),
        json!({"body":json!([history.clone(),body_only.clone()]).to_string()}).to_string(),
    )
    .unwrap();
    assert_eq!(
        success(&h.run(&["issue", "comment", "list", "P-1", "--json"])),
        json!({"items":[history,body_only]})
    );
    std::fs::write(
        h.root.join("response.json"),
        json!({"body":history.to_string()}).to_string(),
    )
    .unwrap();
    assert_eq!(
        success(&h.run(&["issue", "comment", "view", "P-1", "2", "--json"])),
        history
    );
    let mut numeric = comment(3);
    numeric["id"] = serde_json::from_str("123456789012345678901234567890").unwrap();
    std::fs::write(
        h.root.join("response.json"),
        json!({"body":numeric.to_string()}).to_string(),
    )
    .unwrap();
    let output = h
        .command(&["issue", "comment", "view", "P-1", "3", "--json"])
        .env("TEST_KEY", "123456789012345678901234567890")
        .output()
        .unwrap();
    assert_eq!(success(&output)["id"], "[REDACTED]");
    assert_eq!(h.requests().len(), 5);
    assert!(
        h.requests()
            .iter()
            .all(|r| r["path"].as_str().unwrap().contains("/comments"))
    );
}

#[test]
fn fixed_backlog_and_unknown_rules_use_shared_body_display_without_fetching_links() {
    let raw = "* ''日本語'' é 👩‍💻\nP-2\n[[P-2]] #image(11)\n{code}\n''literal'' P-2\n{/code}\n&#27; test-secret";
    let mut value = comment(3);
    value["content"] = json!(raw);
    value["changeLog"] = Value::Null;
    let h = Harness::new(fixture());
    for rule in ["backlog", "future-rule"] {
        let mut response = fixture();
        response["routes"]["/api/v2/issues/P-1/comments/3"]["body"] = json!(value.to_string());
        response["routes"]["/api/v2/projects/987654321098765432109876543210"]["body"] =
            json!(json!({"textFormattingRule":rule}).to_string());
        std::fs::write(h.root.join("response.json"), response.to_string()).unwrap();
        let output = h.run(&["issue", "comment", "view", "P-1", "3"]);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert!(output.stderr.is_empty());
        let text = String::from_utf8(output.stdout).unwrap();
        let expected = if rule == "backlog" {
            format!(
                "日本語 é 👩‍💻\nP-2 ({}/view/P-2)\n[[P-2]] #image(11)\n{{code}}\n''literal'' P-2\n{{/code}}\n&#27; [REDACTED]",
                h.origin
            )
        } else {
            raw.replace("test-secret", "[REDACTED]")
        };
        assert!(text.contains(&format!("BODY\n{expected}\n")), "{text}");
        assert!(!text.contains('\u{1b}'));
    }
    assert_eq!(h.requests().len(), 6);
}

#[test]
fn fixed_issue_and_project_failures_never_emit_partial_comments_and_json_ignores_them() {
    let mut mixed = fixture();
    let mut history = comment(2);
    history["content"] = Value::Null;
    mixed["routes"]["/api/v2/issues/P-1/comments"]["commentItems"] = json!([history, comment(3)]);
    let h = Harness::new(mixed.clone());
    for endpoint in [
        "/api/v2/issues/P-1",
        "/api/v2/projects/987654321098765432109876543210",
    ] {
        for route in [
            json!({"status":403}),
            json!({"body":"{}"}),
            json!({"body":"not json"}),
            json!({"disconnect":true}),
        ] {
            let mut response = mixed.clone();
            response["routes"][endpoint] = route;
            std::fs::write(h.root.join("response.json"), response.to_string()).unwrap();
            for args in [
                vec!["issue", "comment", "list", "P-1"],
                vec!["issue", "comment", "view", "P-1", "3"],
            ] {
                let output = h.run(&args);
                assert_eq!(output.status.code(), Some(1));
                assert!(output.stdout.is_empty());
                assert!(
                    String::from_utf8(output.stderr)
                        .unwrap()
                        .starts_with("error: ")
                );
                let mut args = args;
                args.push("--json");
                success(&h.run(&args));
            }
        }
    }
}

#[test]
fn output_failures_for_list_and_view_are_execution_errors() {
    let h = Harness::new(fixture());
    for args in [
        vec!["issue", "comment", "list", "P-1"],
        vec!["issue", "comment", "view", "P-1", "3"],
    ] {
        for json_mode in [false, true] {
            let mut args = args.clone();
            if json_mode {
                args.push("--json");
            }
            let mut child = h
                .command(&args)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            drop(child.stdout.take());
            let output = child.wait_with_output().unwrap();
            assert_eq!(output.status.code(), Some(1));
            assert!(output.stdout.is_empty());
            if json_mode {
                assert_eq!(failure(&output, 1, "output")["error"]["httpStatus"], 200);
            } else {
                assert_eq!(output.stderr, b"error: Cannot write standard output\n");
            }
        }
    }
}

#[test]
fn comment_help_global_flags_and_tty_keep_existing_noninteractive_contract() {
    let h = Harness::new(fixture());
    std::fs::write(h.root.join("backlog/config.json"), "broken").unwrap();
    for command in ["list", "view"] {
        let output = h.run(&["issue", "comment", command, "--help", "--json"]);
        assert_eq!(output.status.code(), Some(0));
        assert!(output.stderr.is_empty());
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains("<ISSUE_ID_OR_KEY>"));
        assert!(!text.contains("--project"));
        assert!(!text.contains("--offset"));
        assert_eq!(text.contains("--min-id"), command == "list");
        assert_eq!(text.contains("<COMMENT_ID>"), command == "view");
    }
    assert!(h.requests().is_empty());
    h.config(json!({"profiles":{"work":{"spaceUrl":h.origin,"apiKeyEnv":"TEST_KEY"}}}));
    success(&h.run(&[
        "--json",
        "--profile",
        "work",
        "issue",
        "comment",
        "list",
        "P-1",
        "--no-input",
    ]));
    success(&h.run(&[
        "issue",
        "comment",
        "view",
        "P-1",
        "3",
        "--json",
        "--profile",
        "work",
        "--no-input",
    ]));
    for args in [
        vec!["issue", "comment", "list", "--json"],
        vec!["issue", "comment", "view", "P-1", "--json"],
        vec!["--json", "issue", "comment", "list", "P-1", "--json"],
        vec![
            "issue", "comment", "view", "P-1", "3", "--limit", "2", "--json",
        ],
    ] {
        failure(&h.run(&args), 2, "input");
    }
    assert_eq!(h.requests().len(), 2);
    let template = h.command(&[]);
    for (command, no_input) in [("list", false), ("view", true)] {
        let mut args = vec!["issue", "comment", command, "P-1", "--profile", "work"];
        if command == "view" {
            args.push("3");
        }
        if no_input {
            args.push("--no-input");
        }
        let output = std::process::Command::new("python3")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/tty_cli.py"
            ))
            .arg(env!("CARGO_BIN_EXE_bl-cli"))
            .args(args)
            .env_clear()
            .envs(template.get_envs().filter_map(|(k, v)| v.map(|v| (k, v))))
            .env("PATH", std::env::var_os("PATH").unwrap())
            .current_dir(&h.root)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert!(output.stderr.is_empty());
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(
            text.replace("\r\n", "\n")
                .contains("BODY\n日本語 é 👩‍💻\n本文"),
            "{text}"
        );
        assert!(!text.contains('\u{1b}'));
    }
    assert_eq!(h.requests().len(), 8);
}
