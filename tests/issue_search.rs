mod common;
use common::{Harness, success};
use serde_json::{Value, json};

fn fixture() -> Value {
    json!({"routes":{
        "/api/v2/issues/count":{"body":"{\"count\":43}"},
        "/api/v2/issues":{"body":r#"[{"id":123456789012345678901234567890,"issueKey":"P-1","summary":"修正","status":{"id":1,"name":"未対応"},"unknown":true}]"#}
    }})
}
fn query(request: &Value) -> Vec<(String, String)> {
    let url = reqwest::Url::parse(&format!(
        "https://localhost{}",
        request["path"].as_str().unwrap()
    ))
    .unwrap();
    url.query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect()
}
#[test]
fn list_returns_one_page_and_total_with_original_items() {
    let h = Harness::new(fixture());
    let value = success(&h.run(&[
        "issue",
        "list",
        "--project",
        "0007",
        "--limit",
        "2",
        "--offset",
        "8",
        "--json",
    ]));
    assert_eq!(
        value["pagination"],
        json!({"offset":8,"limit":2,"count":1,"total":43})
    );
    assert_eq!(
        value["items"][0]["id"].to_string(),
        "123456789012345678901234567890"
    );
    assert_eq!(value["items"][0]["unknown"], true);
    assert_eq!(value.as_object().unwrap().len(), 2);
    let requests = h.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        query(&requests[0]),
        [
            ("projectId[]".into(), "7".into()),
            ("sort".into(), "updated".into()),
            ("order".into(), "desc".into())
        ]
    );
    let mut expected = query(&requests[0]);
    expected.extend([("count".into(), "2".into()), ("offset".into(), "8".into())]);
    assert_eq!(query(&requests[1]), expected);
}

#[test]
fn normal_search_resolves_project_and_defaults_then_explicit_filters_override() {
    let mut response = fixture();
    response["routes"]["/api/v2/projects/P"] = json!({"body":r#"{"id":7}"#});
    response["routes"]["/api/v2/projects/P/statuses"] =
        json!({"body":r#"[{"id":1,"name":"未対応"},{"id":2,"name":"完了"}]"#});
    response["routes"]["/api/v2/projects/P/users"] =
        json!({"body":r#"[{"id":3,"name":"担当"},{"id":4,"name":"3"}]"#});
    response["routes"]["/api/v2/projects/P/versions"] =
        json!({"body":r#"[{"id":5,"name":"v1"},{"id":6,"name":"v2"}]"#});
    response["routes"]["/api/v2/users/myself"] = json!({"body":r#"{"id":3}"#});
    let h = Harness::new(response);
    std::fs::create_dir(h.root.join(".backlog")).unwrap();
    std::fs::write(h.root.join(".backlog/settings.json"), r#"{"project":"P","issue":{"list":{"status":["未対応","未対応"]},"create":{"assignee":"self","priority":"unused","issueType":"unused"}}}"#).unwrap();
    success(&h.run(&["issue", "list", "--json"]));
    let requests = h.requests();
    let count = requests
        .iter()
        .find(|r| {
            r["path"]
                .as_str()
                .unwrap()
                .starts_with("/api/v2/issues/count?")
        })
        .unwrap();
    assert_eq!(
        query(count),
        [
            ("projectId[]".into(), "7".into()),
            ("statusId[]".into(), "1".into()),
            ("sort".into(), "updated".into()),
            ("order".into(), "desc".into())
        ]
    );
    let start = requests.len();
    success(&h.run(&[
        "issue",
        "list",
        "ログイン &?",
        "-p",
        "P",
        "-s",
        "完了",
        "-s",
        "未対応",
        "-s",
        "完了",
        "-a",
        "担当",
        "-a",
        "3",
        "-a",
        "担当",
        "-a",
        "self",
        "-m",
        "v1",
        "-m",
        "v2",
        "-m",
        "v1",
        "--json",
    ]));
    let requests = h.requests();
    let searches: Vec<_> = requests[start..]
        .iter()
        .filter(|r| r["path"].as_str().unwrap().starts_with("/api/v2/issues"))
        .collect();
    let filters = query(searches[0]);
    assert_eq!(
        filters,
        [
            ("projectId[]", "7"),
            ("keyword", "ログイン &?"),
            ("statusId[]", "2"),
            ("statusId[]", "1"),
            ("assigneeId[]", "3"),
            ("assigneeId[]", "4"),
            ("milestoneId[]", "5"),
            ("milestoneId[]", "6"),
            ("sort", "updated"),
            ("order", "desc")
        ]
        .map(|(k, v)| (k.into(), v.into()))
    );
    assert_eq!(&query(searches[1])[..filters.len()], filters);
    for path in [
        "/api/v2/projects/P/statuses",
        "/api/v2/projects/P/users",
        "/api/v2/projects/P/versions",
        "/api/v2/users/myself",
    ] {
        assert_eq!(
            requests[start..]
                .iter()
                .filter(|r| r["path"] == path)
                .count(),
            1,
            "{path} must be fetched once per search"
        );
    }
    assert!(
        !requests[..start]
            .iter()
            .any(|r| r["path"].as_str().unwrap().contains("myself"))
    );
}

#[test]
fn status_all_clears_defaults_and_direct_ids_are_canonical_sets() {
    let h = Harness::new(fixture());
    std::fs::create_dir(h.root.join(".backlog")).unwrap();
    std::fs::write(
        h.root.join(".backlog/settings.json"),
        r#"{"project":"unused","issue":{"list":{"status":["must-not-resolve"]}}}"#,
    )
    .unwrap();
    success(&h.run(&[
        "issue",
        "list",
        "-p",
        "7",
        "-s",
        "all",
        "-s",
        "all",
        "--assignee-id",
        "0003",
        "--assignee-id",
        "3",
        "--milestone-id",
        "123456789012345678901234567890",
        "--json",
    ]));
    let requests = h.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        query(&requests[0]),
        [
            ("projectId[]", "7"),
            ("assigneeId[]", "3"),
            ("milestoneId[]", "123456789012345678901234567890"),
            ("sort", "updated"),
            ("order", "desc")
        ]
        .map(|(k, v)| (k.into(), v.into()))
    );
    success(&h.run(&[
        "issue",
        "list",
        "-p",
        "7",
        "--status-id",
        "0002",
        "--status-id",
        "2",
        "--status-id",
        "1",
        "--json",
    ]));
    assert_eq!(h.requests().len(), 4);
    assert_eq!(
        query(&h.requests()[2])
            .iter()
            .filter(|(k, _)| k == "statusId[]")
            .map(|(_, v)| v.as_str())
            .collect::<Vec<_>>(),
        ["2", "1"]
    );
}

#[test]
fn mine_and_cross_project_share_search_with_selected_profile_self() {
    let mut response = fixture();
    response["routes"]["/api/v2/users/myself"] =
        json!({"body":r#"{"id":987654321098765432109876543210,"name":"自分"}"#});
    response["routes"]["/api/v2/issues"]["body"] = json!(
        r#"[{"id":1,"projectId":7,"issueKey":"P-1","summary":"one","status":{"name":"open"}},{"id":2,"projectId":8,"issueKey":"Q-2","summary":"two","status":{"name":"open"}}]"#
    );
    let h = Harness::new(response);
    h.config(json!({"profiles":{"work":{"spaceUrl":h.origin,"apiKeyEnv":"WORK_KEY"}}}));
    std::fs::create_dir(h.root.join(".backlog")).unwrap();
    std::fs::write(h.root.join(".backlog/settings.json"), r#"{"project":"unused","issue":{"list":{"status":["never"]},"create":{"assignee":"never"}}}"#).unwrap();
    for args in [
        vec!["--profile", "work", "issue", "mine", "-p", "all", "--json"],
        vec![
            "issue",
            "list",
            "-p",
            "all",
            "-a",
            "self",
            "-a",
            "self",
            "--profile",
            "work",
            "--json",
        ],
        vec![
            "issue",
            "list",
            " 横断 ",
            "-p",
            "all",
            "--profile",
            "work",
            "--json",
        ],
        vec![
            "issue",
            "mine",
            "-p",
            "7",
            "-s",
            "all",
            "--profile",
            "work",
            "--json",
        ],
    ] {
        let value = success(
            &h.command(&args)
                .env("WORK_KEY", "profile-secret")
                .output()
                .unwrap(),
        );
        assert_eq!(value["items"].as_array().unwrap().len(), 2);
    }
    let requests = h.requests();
    assert_eq!(requests.len(), 11);
    let counts: Vec<_> = requests
        .iter()
        .filter(|r| {
            r["path"]
                .as_str()
                .unwrap()
                .starts_with("/api/v2/issues/count?")
        })
        .collect();
    for count in &counts[..2] {
        assert_eq!(
            query(count),
            [
                ("assigneeId[]", "987654321098765432109876543210"),
                ("sort", "updated"),
                ("order", "desc")
            ]
            .map(|(k, v)| (k.into(), v.into()))
        );
    }
    assert_eq!(
        query(counts[2]),
        [
            ("keyword", " 横断 "),
            ("sort", "updated"),
            ("order", "desc")
        ]
        .map(|(k, v)| (k.into(), v.into()))
    );
    assert!(requests.iter().all(|r| {
        r["headers"]
            .as_object()
            .unwrap()
            .iter()
            .any(|(k, v)| k.eq_ignore_ascii_case("Backlog-API-Key") && v == "profile-secret")
    }));
}

#[test]
fn invalid_inputs_and_forbidden_cross_project_filters_fail_before_http() {
    let h = Harness::new(json!({"status":500}));
    for flags in [
        vec![],
        vec!["-p", "7", "--limit", "0"],
        vec!["-p", "7", "--limit", "101"],
        vec!["-p", "7", "--limit", "999"],
        vec!["-p", "7", "--offset=-1"],
        vec!["-p", "7", "--offset", "1.0"],
        vec!["-p", "0"],
        vec!["-p", "7", "-p", "8"],
        vec!["-p", "7", "-s", "all", "-s", "open"],
        vec!["-p", "7", "-s", "all", "--status-id", "1"],
        vec!["-p", "7", "-a", "self", "--assignee-id", "1"],
        vec!["-p", "7", "-m", "v1", "--milestone-id", "1"],
        vec!["-p", "all"],
        vec!["  ", "-p", "all"],
        vec!["term", "-p", "all", "-s", "all"],
        vec!["term", "-p", "all", "--status-id", "1"],
        vec!["term", "-p", "all", "-m", "v1"],
        vec!["term", "-p", "all", "--milestone-id", "1"],
        vec!["term", "-p", "all", "-a", "other"],
        vec!["term", "-p", "all", "--assignee-id", "1"],
        vec!["term", "-p", "all", "-a", "self", "-a", "other"],
    ] {
        let mut args = vec!["issue", "list", "--json"];
        args.extend(flags);
        common::failure(&h.run(&args), 2, "input");
    }
    for flag in ["--status-id", "--assignee-id", "--milestone-id"] {
        for id in ["", "0", "000", "+1", "-1", "1.0", "1e2", " 1", "１"] {
            let input = format!("{flag}={id}");
            common::failure(
                &h.run(&["issue", "list", "-p", "P", &input, "--json"]),
                2,
                "input",
            );
        }
    }
    for flags in [
        vec!["-a", "self"],
        vec!["--assignee-id", "1"],
        vec!["--assignee", "other"],
        vec!["-p", "all", "-s", "all"],
        vec!["-p", "all", "--milestone-id", "1"],
    ] {
        let mut args = vec!["issue", "mine", "--json"];
        args.extend(flags);
        common::failure(&h.run(&args), 2, "input");
    }
    assert!(h.requests().is_empty());
}

#[test]
fn json_safety_failure_retains_response_status_and_never_emits_success() {
    let mut response = fixture();
    response["routes"]["/api/v2/issues"]["body"] = json!(
        r#"[{"id":1,"issueKey":"P-1","summary":"x","status":{"name":"open"},"test-secret":"x","[REDACTED]":"y"}]"#
    );
    let h = Harness::new(response);
    let value = common::failure(&h.run(&["issue", "list", "-p", "7", "--json"]), 1, "output");
    assert_eq!(value["error"]["httpStatus"], 200);
    assert!(!value.to_string().contains("test-secret"));
}

#[test]
fn exact_missing_and_ambiguous_names_are_input_errors_not_empty_searches() {
    let mut response = fixture();
    for endpoint in ["statuses", "users", "versions"] {
        response["routes"][format!("/api/v2/projects/7/{endpoint}")] = json!({"body":r#"[{"id":1,"name":"duplicate"},{"id":2,"name":"duplicate"},{"id":3,"name":"exact"}]"#});
    }
    let h = Harness::new(response);
    for flag in ["-s", "-a", "-m"] {
        for name in ["duplicate", "missing", "exact ", "EXACT", ""] {
            common::failure(
                &h.run(&["issue", "list", "-p", "7", flag, name, "--json"]),
                2,
                "input",
            );
        }
    }
    assert_eq!(h.requests().len(), 15);
    assert!(
        h.requests()
            .iter()
            .all(|r| !r["path"].as_str().unwrap().contains("/issues"))
    );
}

#[test]
fn empty_pages_and_snapshot_discrepancies_are_successful_without_extra_paging() {
    for (body, total, expected_count) in [
        ("[]", "99", 0),
        ("[]", "0", 0),
        (
            r#"[{"id":1,"issueKey":"P-1","summary":"one","status":{"name":"open"}}]"#,
            "0",
            1,
        ),
    ] {
        let mut response = fixture();
        response["routes"]["/api/v2/issues/count"]["body"] =
            json!(format!("{{\"count\":{total}}}"));
        response["routes"]["/api/v2/issues"]["body"] = json!(body);
        let h = Harness::new(response);
        let value = success(&h.run(&[
            "issue", "list", "-p", "7", "--offset", "0008", "--limit", "100", "--json",
        ]));
        assert_eq!(
            value["pagination"],
            json!({"offset":8,"limit":100,"count":expected_count,"total":total.parse::<u32>().unwrap()})
        );
        assert_eq!(h.requests().len(), 2);
        if expected_count == 0 {
            let output = h.run(&["issue", "list", "-p", "7"]);
            assert_eq!(output.status.code(), Some(0));
            assert!(output.stderr.is_empty());
            assert_eq!(output.stdout, b"ID\tKEY\tTITLE\tSTATUS\n");
        }
    }
}

#[test]
fn count_list_project_and_resolution_errors_propagate_without_success_output() {
    for (endpoint, args, expected_requests) in [
        ("/api/v2/issues/count", vec!["-p", "7"], 1),
        ("/api/v2/issues", vec!["-p", "7"], 2),
        ("/api/v2/projects/P", vec!["-p", "P"], 1),
        (
            "/api/v2/projects/7/statuses",
            vec!["-p", "7", "-s", "open"],
            1,
        ),
        ("/api/v2/users/myself", vec!["-p", "all", "-a", "self"], 1),
    ] {
        let mut response = fixture();
        response["routes"][endpoint] = json!({"status":403,"body":r#"{"errors":[{"code":5,"message":"test-secret\u001b[31m"}]}"#});
        let h = Harness::new(response);
        let mut command = vec!["issue", "list", "--json"];
        command.extend(args);
        let error = common::failure(&h.run(&command), 1, "permission");
        assert_eq!(error["error"]["httpStatus"], 403);
        assert_eq!(error["error"]["apiErrors"][0]["code"], 5);
        assert!(!error.to_string().contains("test-secret"));
        assert_eq!(h.requests().len(), expected_requests);
    }
}

#[test]
fn invalid_count_and_issue_responses_are_not_empty_successes() {
    for (endpoint, bodies, expected_requests) in [
        (
            "/api/v2/issues/count",
            vec![
                "not json",
                "[]",
                "{}",
                "{\"count\":-1}",
                "{\"count\":1.5}",
                "{\"count\":\"1\"}",
                "null",
            ],
            1,
        ),
        (
            "/api/v2/issues",
            vec![
                "not json",
                "{}",
                "[null]",
                "[{}]",
                r#"[{"id":0,"issueKey":"P-1","summary":"x","status":{"name":"open"}}]"#,
                r#"[{"id":1,"issueKey":"P-1","summary":"x","status":null}]"#,
            ],
            2,
        ),
        (
            "/api/v2/projects/P",
            vec!["{}", "{\"id\":0}", "{\"id\":\"7\"}"],
            1,
        ),
    ] {
        for body in bodies {
            let mut response = fixture();
            response["routes"][endpoint] = json!({"body":body});
            let h = Harness::new(response);
            let project = if endpoint == "/api/v2/projects/P" {
                "P"
            } else {
                "7"
            };
            let error = common::failure(
                &h.run(&["issue", "list", "-p", project, "--json"]),
                1,
                "invalid_response",
            );
            assert_eq!(error["error"]["httpStatus"], 200);
            assert_eq!(h.requests().len(), expected_requests);
        }
    }
}

#[test]
fn search_help_bypasses_configuration_and_mine_does_not_offer_assignee_flags() {
    let h = Harness::new(json!({"status":500}));
    std::fs::write(h.root.join("backlog/config.json"), "broken").unwrap();
    for command in ["list", "mine"] {
        let output = h.run(&["issue", command, "--help", "--json"]);
        assert_eq!(output.status.code(), Some(0));
        assert!(output.stderr.is_empty());
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains("--status-id"));
        assert_eq!(text.contains("--assignee"), command == "list");
    }
    assert!(h.requests().is_empty());
}

#[test]
fn search_json_preserves_unknown_fields_huge_numbers_and_redacts_secrets() {
    let mut response = fixture();
    response["routes"]["/api/v2/issues/count"]["body"] =
        json!(r#"{"count":123456789012345678901234567890}"#);
    response["routes"]["/api/v2/issues"]["body"] = json!(
        r#"[{"id":1,"issueKey":"P-1","summary":"日本語 é 👩‍💻\n\t test-secret\u001b[31m","status":{"name":"open"},"test-secret-field":["test-secret",123456789012345678901234567890],"description":" original\n "}]"#
    );
    let h = Harness::new(response);
    let output = h.run(&["issue", "list", "-p", "7", "--json"]);
    let value = success(&output);
    assert_eq!(
        value["pagination"]["total"].to_string(),
        "123456789012345678901234567890"
    );
    assert_eq!(
        value["items"][0]["[REDACTED]-field"][1].to_string(),
        "123456789012345678901234567890"
    );
    assert_eq!(value["items"][0]["description"], " original\n ");
    assert!(
        value["items"][0]["summary"]
            .as_str()
            .unwrap()
            .contains("日本語 é 👩‍💻\n\t")
    );
    assert!(!value.to_string().contains("test-secret"));
    assert!(
        !value["items"][0]["summary"]
            .as_str()
            .unwrap()
            .contains('\u{1b}')
    );
    assert_eq!(output.stdout.last(), Some(&b'\n'));
}

#[test]
fn list_and_mine_complete_on_real_tty_without_input_with_or_without_no_input() {
    let mut response = fixture();
    response["routes"]["/api/v2/users/myself"] = json!({"body":"{\"id\":3}"});
    response["routes"]["/api/v2/issues"]["body"] = json!(
        r#"[{"id":1,"issueKey":"P-1","summary":"test-secret\u001b[31m","status":{"name":"open"}}]"#
    );
    let h = Harness::new(response);
    let template = h.command(&[]);
    for command in ["list", "mine"] {
        for no_input in [false, true] {
            let mut args = vec!["issue", command, "-p", "7"];
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
            assert!(text.starts_with("ID\tKEY\tTITLE\tSTATUS"));
            assert!(text.contains("1\tP-1\t[REDACTED]"));
            assert!(text.contains("open"));
            assert!(!text.contains("test-secret"));
            assert!(!text.contains('\u{1b}'));
        }
    }
    assert_eq!(h.requests().len(), 10);
}
