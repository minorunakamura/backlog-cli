mod common;
use common::{Harness, success};
use serde_json::{Value, json};

fn issue() -> Value {
    serde_json::from_str(r##"{"id":123456789012345678901234567890,"projectId":987654321098765432109876543210,"issueKey":"P_2-1","summary":"修正 é 👩‍💻","description":"# 見出し\n<b>&#27;原文</b>\t本文","issueType":{"name":"タスク"},"priority":{"name":"中"},"assignee":{"name":"担当"},"status":{"name":"完了"},"resolution":{"id":0,"name":"対応済み"},"startDate":"2026-10-01T00:00:00Z","dueDate":"2026-11-01T00:00:00Z","milestone":[{"name":"v1"},{"name":"v2"}],"created":"2026-10-01T01:02:03Z","updated":"2026-10-02T04:05:06Z","unknown":{"number":123456789012345678901234567890,"text":"  unchanged\n\t<&>  "}}"##).unwrap()
}
fn fixture(value: &Value) -> Value {
    json!({"routes":{
        "/api/v2/issues/P_2-1":{"body":value.to_string()},
        "/api/v2/projects/987654321098765432109876543210":{"body":"{\"textFormattingRule\":\"markdown\"}"}
    }})
}
#[test]
fn json_view_preserves_original_fields_and_builds_url_without_project_get() {
    let mut original = issue();
    original["url"] = json!("https://evil.invalid/ignored?test-secret");
    let h = Harness::new(fixture(&original));
    let value = success(&h.run(&["issue", "view", "P_2-1", "--json"]));
    let mut expected = original;
    expected["url"] = json!(format!("{}/view/P_2-1", h.origin));
    assert_eq!(value, expected);
    let requests = h.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0]["path"], "/api/v2/issues/P_2-1");
    assert_eq!(requests[0]["method"], "GET");
    assert_eq!(requests[0]["headers"]["backlog-api-key"], "test-secret");
}

#[test]
fn fixed_view_shows_every_detail_and_markdown_body_using_the_issue_project() {
    let h = Harness::new(fixture(&issue()));
    std::fs::create_dir(h.root.join(".backlog")).unwrap();
    std::fs::write(
        h.root.join(".backlog/settings.json"),
        r#"{"project":"IGNORED"}"#,
    )
    .unwrap();
    let output = h.run(&["--no-input", "issue", "view", "P_2-1"]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stderr.is_empty());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!(
            "KEY\tP_2-1\nTITLE\t修正 é 👩‍💻\nTYPE\tタスク\nPRIORITY\t中\nASSIGNEE\t担当\nSTATUS\t完了\nRESOLUTION\t対応済み\nSTART DATE\t2026-10-01T00:00:00Z\nDUE DATE\t2026-11-01T00:00:00Z\nMILESTONES\tv1, v2\nCREATED\t2026-10-01T01:02:03Z\nUPDATED\t2026-10-02T04:05:06Z\nURL\t{}/view/P_2-1\nBODY\n見出し\n<b>\\u{{1b}}原文</b>\t本文\n",
            h.origin
        )
    );
    let requests = h.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[1]["path"],
        "/api/v2/projects/987654321098765432109876543210"
    );
}

#[test]
fn invalid_issue_identifiers_are_input_errors_without_requests() {
    let h = Harness::new(fixture(&issue()));
    for id in [
        "",
        "0",
        "000",
        "-1",
        "1.0",
        "P",
        "P-0",
        "P-",
        "-2",
        ".",
        "..",
        "../P-1",
        "P/1-2",
        "P-1?x",
        "https://evil.test/P-1",
        "P-1#x",
        "P%2F-1",
        "P-1\n",
    ] {
        let error = common::failure(&h.run(&["issue", "view", "--json", "--", id]), 2, "input");
        assert!(error["error"]["httpStatus"].is_null(), "{id}: {error}");
    }
    assert!(h.requests().is_empty());
}

#[test]
fn invalid_required_issue_fields_fail_before_any_success_output() {
    let h = Harness::new(fixture(&issue()));
    for field in [
        "id",
        "projectId",
        "issueKey",
        "summary",
        "description",
        "issueType",
        "priority",
        "assignee",
        "status",
        "resolution",
        "startDate",
        "dueDate",
        "milestone",
        "created",
        "updated",
    ] {
        for replacement in [None, Some(json!(true))] {
            let mut value = issue();
            if let Some(replacement) = replacement {
                value[field] = replacement;
            } else {
                value.as_object_mut().unwrap().remove(field);
            }
            std::fs::write(h.root.join("response.json"), fixture(&value).to_string()).unwrap();
            let error = common::failure(
                &h.run(&["issue", "view", "P_2-1", "--json"]),
                1,
                "invalid_response",
            );
            assert_eq!(error["error"]["httpStatus"], 200, "{field}");
        }
    }
    for (field, bad) in [
        ("id", json!(0)),
        ("projectId", json!(-1)),
        ("projectId", json!(1.5)),
        ("issueKey", json!("../P-1")),
        ("issueKey", json!("P-1?x")),
        ("issueKey", json!("https://evil.test/P-1")),
        ("issueKey", json!("P-0")),
        ("issueType", json!({})),
        ("assignee", json!({"name":1})),
        ("resolution", json!({"name":null})),
        ("status", json!("完了")),
        ("milestone", json!([{"name":"v1"},null])),
        ("milestone", json!([{"name":3}])),
    ] {
        let mut value = issue();
        value[field] = bad;
        std::fs::write(h.root.join("response.json"), fixture(&value).to_string()).unwrap();
        let error = common::failure(
            &h.run(&["issue", "view", "P_2-1", "--json"]),
            1,
            "invalid_response",
        );
        assert_eq!(error["error"]["httpStatus"], 200, "{field}");
    }
    for body in ["[]", "null", "1", "{}", "not json"] {
        std::fs::write(
            h.root.join("response.json"),
            json!({"body":body}).to_string(),
        )
        .unwrap();
        let error = common::failure(
            &h.run(&["issue", "view", "P_2-1", "--json"]),
            1,
            "invalid_response",
        );
        assert_eq!(error["error"]["httpStatus"], 200);
    }
    assert!(
        h.requests()
            .iter()
            .all(|r| r["path"] == "/api/v2/issues/P_2-1")
    );
}

#[test]
fn nullable_details_empty_milestones_and_unknown_rules_keep_safe_raw_fallback() {
    let mut value = issue();
    for field in [
        "description",
        "assignee",
        "resolution",
        "startDate",
        "dueDate",
    ] {
        value[field] = Value::Null;
    }
    value["milestone"] = json!([]);
    let h = Harness::new(fixture(&value));
    let returned = success(&h.run(&["issue", "view", "P_2-1", "--json"]));
    for field in [
        "description",
        "assignee",
        "resolution",
        "startDate",
        "dueDate",
    ] {
        assert!(returned[field].is_null());
    }
    assert_eq!(returned["milestone"], json!([]));
    let output = h.run(&["issue", "view", "P_2-1"]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let text = String::from_utf8(output.stdout).unwrap();
    for label in ["ASSIGNEE", "RESOLUTION", "START DATE", "DUE DATE"] {
        assert!(text.contains(&format!("{label}\t-\n")));
    }
    assert!(text.contains("MILESTONES\t\n"));
    assert!(text.ends_with("BODY\n\n"));
    value["description"] = json!(
        " * 原文\n```\n<b>&#27;</b> [[macro]]\n```\t\u{1b}]52;c;clipboard\u{7}\r\u{9b}test-secret"
    );
    value["summary"] = json!("test-secret\u{1b}[31m");
    for rule in ["backlog", "future-rule", ""] {
        let mut response = fixture(&value);
        response["routes"]["/api/v2/projects/987654321098765432109876543210"]["body"] =
            json!(json!({"textFormattingRule":rule}).to_string());
        std::fs::write(h.root.join("response.json"), response.to_string()).unwrap();
        let output = h.run(&["issue", "view", "P_2-1"]);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert!(output.stderr.is_empty());
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains("TITLE\t[REDACTED]\\u{1b}[31m\n"));
        assert!(text.ends_with("BODY\n * 原文\n```\n<b>&#27;</b> [[macro]]\n```\t\\u{1b}]52;c;clipboard\\u{7}\\u{d}\\u{9b}[REDACTED]\n"), "{text}");
        assert!(!text.contains("test-secret"));
        assert!(!text.contains('\u{1b}'));
    }
}

#[test]
fn json_redacts_unknown_keys_arrays_and_scalars_and_rejects_key_collisions() {
    let mut value = issue();
    value["test-secret-field"] =
        json!(["test-secret", {"nested-test-secret":"body test-secret\u{1b}[31m"}]);
    let h = Harness::new(fixture(&value));
    let output = h.run(&["issue", "view", "P_2-1", "--json"]);
    let result = success(&output);
    assert_eq!(result["[REDACTED]-field"][0], "[REDACTED]");
    assert_eq!(
        result["[REDACTED]-field"][1]["nested-[REDACTED]"],
        "body [REDACTED]\\u{1b}[31m"
    );
    assert!(
        !String::from_utf8(output.stdout)
            .unwrap()
            .contains("test-secret")
    );
    value["[REDACTED]-field"] = json!("collision");
    std::fs::write(h.root.join("response.json"), fixture(&value).to_string()).unwrap();
    let error = common::failure(&h.run(&["issue", "view", "P_2-1", "--json"]), 1, "output");
    assert_eq!(error["error"]["httpStatus"], 200);
    assert!(!error.to_string().contains("test-secret"));
    for (secret, scalar) in [
        ("123456789012345678901234567890", issue()["id"].clone()),
        ("true", json!(true)),
        ("null", Value::Null),
    ] {
        let mut value = issue();
        value["unknownScalar"] = json!([scalar]);
        std::fs::write(h.root.join("response.json"), fixture(&value).to_string()).unwrap();
        let output = h
            .command(&["issue", "view", "P_2-1", "--json"])
            .env("TEST_KEY", secret)
            .output()
            .unwrap();
        let result = success(&output);
        assert_eq!(result["unknownScalar"][0], "[REDACTED]");
        assert!(!String::from_utf8(output.stdout).unwrap().contains(secret));
    }
    assert_eq!(h.requests().len(), 5);
    assert!(
        h.requests()
            .iter()
            .all(|r| r["path"] == "/api/v2/issues/P_2-1")
    );
}

#[test]
fn numeric_ids_are_not_narrowed_and_url_uses_response_key_without_membership_checks() {
    let original = issue();
    let h = Harness::new(json!({"body":original.to_string()}));
    for id in ["0007", "123456789012345678901234567890"] {
        let result = success(&h.run(&["issue", "view", id, "--json"]));
        assert_eq!(result["url"], format!("{}/view/P_2-1", h.origin));
        assert_eq!(
            result["projectId"].to_string(),
            "987654321098765432109876543210"
        );
    }
    let requests = h.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0]["path"], "/api/v2/issues/0007");
    assert_eq!(
        requests[1]["path"],
        "/api/v2/issues/123456789012345678901234567890"
    );
}

#[test]
fn issue_and_project_failures_preserve_error_kind_status_and_empty_stdout() {
    let h = Harness::new(fixture(&issue()));
    for endpoint in [
        "/api/v2/issues/P_2-1",
        "/api/v2/projects/987654321098765432109876543210",
    ] {
        for (status, kind) in [
            (401, "auth"),
            (403, "permission"),
            (404, "not_found"),
            (429, "rate_limit"),
            (500, "api"),
            (302, "api"),
        ] {
            let mut response = fixture(&issue());
            response["routes"][endpoint] = json!({"status":status,"location":"https://evil.invalid/","body":"{\"errors\":[{\"code\":5,\"message\":\"test-secret\\u001b[31m\"}]}"});
            std::fs::write(h.root.join("response.json"), response.to_string()).unwrap();
            // Project failures are exercised in fixed mode; JSON must ignore that endpoint.
            let output = h.run(&["issue", "view", "P_2-1"]);
            assert_eq!(output.status.code(), Some(1), "{output:?}");
            assert!(output.stdout.is_empty());
            assert!(
                !String::from_utf8(output.stderr)
                    .unwrap()
                    .contains("test-secret")
            );
            if endpoint.contains("/issues/") {
                let error = common::failure(&h.run(&["issue", "view", "P_2-1", "--json"]), 1, kind);
                assert_eq!(error["error"]["httpStatus"], status);
                assert_eq!(error["error"]["apiErrors"][0]["code"], 5);
                assert!(!error.to_string().contains("test-secret"));
            } else {
                success(&h.run(&["issue", "view", "P_2-1", "--json"]));
            }
        }
    }
}

#[test]
fn malformed_project_or_network_failure_never_emits_partial_detail() {
    let h = Harness::new(fixture(&issue()));
    for route in [
        json!({"body":"{}"}),
        json!({"body":"{\"textFormattingRule\":null}"}),
        json!({"body":"not json"}),
        json!({"disconnect":true}),
    ] {
        let mut response = fixture(&issue());
        response["routes"]["/api/v2/projects/987654321098765432109876543210"] = route;
        std::fs::write(h.root.join("response.json"), response.to_string()).unwrap();
        let output = h.run(&["issue", "view", "P_2-1"]);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert!(
            String::from_utf8(output.stderr)
                .unwrap()
                .starts_with("error: ")
        );
    }
    std::fs::write(
        h.root.join("response.json"),
        json!({"disconnect":true}).to_string(),
    )
    .unwrap();
    let error = common::failure(&h.run(&["issue", "view", "P_2-1", "--json"]), 1, "network");
    assert!(error["error"]["httpStatus"].is_null());
}

#[test]
fn output_failures_are_execution_errors_in_both_modes() {
    let h = Harness::new(fixture(&issue()));
    for json_mode in [false, true] {
        let mut args = vec!["issue", "view", "P_2-1"];
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
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        assert!(output.stdout.is_empty());
        if json_mode {
            let error = common::failure(&output, 1, "output");
            assert_eq!(error["error"]["httpStatus"], 200);
        } else {
            assert_eq!(output.stderr, b"error: Cannot write standard output\n");
        }
    }
}

#[test]
fn view_help_and_flags_preserve_noninteractive_cli_contract() {
    let h = Harness::new(fixture(&issue()));
    std::fs::write(h.root.join("backlog/config.json"), "broken").unwrap();
    let output = h.run(&["issue", "view", "--help", "--json"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("<ISSUE_ID_OR_KEY>"));
    assert!(!text.contains("--project"));
    assert!(h.requests().is_empty());
    h.config(json!({"profiles":{"work":{"spaceUrl":h.origin,"apiKeyEnv":"TEST_KEY"}}}));
    success(&h.run(&[
        "--json",
        "--profile",
        "work",
        "issue",
        "view",
        "P_2-1",
        "--no-input",
    ]));
    success(&h.run(&[
        "issue",
        "view",
        "P_2-1",
        "--no-input",
        "--json",
        "--profile",
        "work",
    ]));
    for args in [
        vec!["issue", "view", "--json"],
        vec!["issue", "view", "P_2-1", "-p", "P", "--json"],
        vec!["--json", "issue", "view", "P_2-1", "--json"],
    ] {
        common::failure(&h.run(&args), 2, "input");
    }
    assert_eq!(h.requests().len(), 2);
    let template = h.command(&[]);
    for no_input in [false, true] {
        let mut args = vec!["issue", "view", "P_2-1", "--profile", "work"];
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
        assert!(text.starts_with("KEY\tP_2-1"));
        assert!(text.contains("見出し"));
        assert!(!text.contains("# 見出し"));
        assert!(text.contains("<b>\\u{1b}原文</b>"));
        assert!(!text.contains('\u{1b}'));
    }
    assert_eq!(h.requests().len(), 6);
}

#[test]
fn markdown_view_is_plain_and_safe_while_json_keeps_the_body_without_project_get() {
    let raw = "# **日本語** é 👩‍💻\n\n- [x] 完了\n\n> 引用 *本文*\n\n[参照](https://example.invalid/test&#45;secret)\n![代替](https://example.invalid/image.png)\n\n| 名前 | 値 |\n| --- | --- |\n| a | `**code**` |\n\n&#27;]52;c;bad&#7; test&#45;secret\n[[unknown]]\n";
    let mut value = issue();
    value["description"] = json!(raw);
    let h = Harness::new(fixture(&value));
    let output = h.run(&["issue", "view", "P_2-1"]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stderr.is_empty());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.ends_with("BODY\n日本語 é 👩‍💻\n\n• ☑ 完了\n\n> 引用 本文\n\n参照 (https://example.invalid/[REDACTED])\n代替 (https://example.invalid/image.png)\n\n名前\t値\na\t**code**\n\n\\u{1b}]52;c;bad\\u{7} [REDACTED]\n[[unknown]]\n\n"), "{text}");
    assert!(!text.contains('\u{1b}'));
    assert!(!text.contains("test-secret"));
    assert_eq!(
        h.requests().len(),
        2,
        "images and links must not be fetched"
    );
    let returned = success(&h.run(&["issue", "view", "P_2-1", "--json"]));
    assert_eq!(returned["description"], raw);
    let requests = h.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[2]["path"], "/api/v2/issues/P_2-1");
}

#[test]
fn backlog_view_uses_configured_origin_without_fetching_references_and_json_stays_raw() {
    let raw = "* ''日本語'' é 👩‍💻\n- [x] 完了\nBLG-104 [[P_2-87]]\n[[公式>https://example.invalid/help]]\n#image(11) #attach(sample.zip:11) #rev(abc123) [[Wiki ページ]]\n|~項目|値|h\n|a|''test''-secret|\n{code:java}\nBLG-104 ''literal''\n{/code}\n\\BLG-104 \\[[P_2-87]]\n> 引用 %%本文%%\n{code}one{/code} {code}\n* ''literal'' BLG-104\n{/code}\n|a|b|\n|{code}|x|\n* ''literal'' BLG-104\n{/code}\n{quote}#unknown(''raw'' BLG-1){/quote}\n'''literal'' BLG-104\n* ''後続'' BLG-104\n\u{1b}]52;c;bad\u{7}\n";
    let mut value = issue();
    value["description"] = json!(raw);
    let mut response = fixture(&value);
    response["routes"]["/api/v2/projects/987654321098765432109876543210"]["body"] =
        json!("{\"textFormattingRule\":\"backlog\"}");
    let h = Harness::new(response);
    let output = h.run(&["issue", "view", "P_2-1"]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stderr.is_empty());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.ends_with(&format!("BODY\n日本語 é 👩‍💻\n• ☑ 完了\nBLG-104 ({0}/view/BLG-104) P_2-87 ({0}/view/P_2-87)\n公式 (https://example.invalid/help)\n#image(11) #attach(sample.zip:11) #rev(abc123) [[Wiki ページ]]\n項目\t値\na\t[REDACTED]\n{{code:java}}\nBLG-104 ''literal''\n{{/code}}\n\\BLG-104 \\[[P_2-87]]\n> 引用 本文\n{{code}}one{{/code}} {{code}}\n* ''literal'' BLG-104\n{{/code}}\n|a|b|\n|{{code}}|x|\n* ''literal'' BLG-104\n{{/code}}\n{{quote}}#unknown(''raw'' BLG-1){{/quote}}\n'''literal'' BLG-104\n後続 BLG-104 ({0}/view/BLG-104)\n\\u{{1b}}]52;c;bad\\u{{7}}\n\n", h.origin)), "{text}");
    assert!(!text.contains("test-secret"));
    assert!(!text.contains('\u{1b}'));
    let returned = success(&h.run(&["issue", "view", "P_2-1", "--json"]));
    let mut expected = value;
    expected["url"] = json!(format!("{}/view/P_2-1", h.origin));
    expected["description"] = json!(raw.replace('\u{1b}', "\\u{1b}").replace('\u{7}', "\\u{7}"));
    assert_eq!(returned, expected);
    let requests = h.requests();
    assert_eq!(requests.len(), 3, "no image, reference, or issue-link GETs");
    assert_eq!(requests[0]["path"], "/api/v2/issues/P_2-1");
    assert_eq!(
        requests[1]["path"],
        "/api/v2/projects/987654321098765432109876543210"
    );
    assert_eq!(requests[2]["path"], "/api/v2/issues/P_2-1");
    assert!(requests.iter().all(|r| r["method"] == "GET"));
}
