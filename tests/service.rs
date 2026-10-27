use backlog_cli::{
    config::{Config, Settings},
    service::{Candidate, Selection, Service},
};
use serde_json::json;
use std::process::Command;
mod common;
use common::Harness;

// A separate process gives each library client its own test-only TLS trust/environment.
fn library_process(name: &str, response: serde_json::Value) -> bool {
    if std::env::var_os("SERVICE_TEST").is_some() {
        return false;
    }
    let h = Harness::new(response);
    let template = h.command(&[]);
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env_clear()
        .envs(
            template
                .get_envs()
                .filter_map(|(key, value)| value.map(|v| (key, v))),
        )
        .env("SERVICE_TEST", &h.origin)
        .env("SERVICE_ROOT", &h.root)
        .current_dir(&h.root)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    true
}
fn service() -> Service {
    Service::new(&Config {
        origin: std::env::var("SERVICE_TEST").unwrap().parse().unwrap(),
        api_key: std::env::var("TEST_KEY").unwrap(),
        editor: vec!["vim".into()],
        settings: Settings::default(),
    })
    .unwrap()
}
fn requests() -> Vec<serde_json::Value> {
    std::fs::read_to_string(
        std::path::Path::new(&std::env::var("SERVICE_ROOT").unwrap()).join("requests"),
    )
    .unwrap_or_default()
    .lines()
    .map(|s| serde_json::from_str(s).unwrap())
    .collect()
}
#[test]
fn priority_resolution_uses_exact_names_and_preserves_large_ids() {
    if library_process(
        "priority_resolution_uses_exact_names_and_preserves_large_ids",
        json!({"body":r#"[{"id":123456789012345678901234567890,"name":"3"},{"id":4,"name":"中"}]"#}),
    ) {
        return;
    }
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let s = service();
        assert_eq!(
            s.resolve(Candidate::Priority, Selection::Name("3"))
                .await
                .unwrap()
                .to_string(),
            "123456789012345678901234567890"
        );
        for name in ["中 ", "missing"] {
            let error = s
                .resolve(Candidate::Priority, Selection::Name(name))
                .await
                .unwrap_err();
            assert_eq!(error.kind, "input");
            assert_eq!(error.exit_code(), 2);
        }
        assert_eq!(requests().len(), 3);
        assert!(requests().iter().all(|r| r["path"] == "/api/v2/priorities"));
    });
}

#[test]
fn direct_ids_validate_without_any_candidate_request() {
    if library_process(
        "direct_ids_validate_without_any_candidate_request",
        json!({"status":500}),
    ) {
        return;
    }
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let s = service();
        for candidate in candidates() {
            for id in ["1", "0007", "123456789012345678901234567890"] {
                let expected = if id == "0007" { "7" } else { id };
                assert_eq!(
                    s.resolve(candidate, Selection::Id(id))
                        .await
                        .unwrap()
                        .to_string(),
                    expected
                );
            }
            for id in ["", "0", "-1", "+1", " 1", "1.0", "1e2", "１"] {
                if id == "0" && matches!(candidate, Candidate::Resolution) {
                    assert_eq!(
                        s.resolve(candidate, Selection::Id(id))
                            .await
                            .unwrap()
                            .to_string(),
                        "0"
                    );
                } else {
                    let e = s.resolve(candidate, Selection::Id(id)).await.unwrap_err();
                    assert_eq!((e.kind, e.exit_code()), ("input", 2));
                }
            }
        }
        assert!(requests().is_empty());
    });
}

#[test]
fn candidates_use_only_the_requested_endpoint_without_cache() {
    if library_process(
        "candidates_use_only_the_requested_endpoint_without_cache",
        json!({"body":r#"[{"id":7,"name":"候補","extra":true}]"#}),
    ) {
        return;
    }
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let s = service();
        for (candidate, path) in [
            (
                Candidate::IssueType { project: "P" },
                "/api/v2/projects/P/issueTypes",
            ),
            (
                Candidate::Status { project: "42" },
                "/api/v2/projects/42/statuses",
            ),
            (
                Candidate::Assignee { project: "P" },
                "/api/v2/projects/P/users",
            ),
            (
                Candidate::Milestone { project: "P" },
                "/api/v2/projects/P/versions",
            ),
            (Candidate::Priority, "/api/v2/priorities"),
            (Candidate::Resolution, "/api/v2/resolutions"),
        ] {
            for _ in 0..2 {
                let items = s.candidates(candidate).await.unwrap();
                assert_eq!(items, json!([{"id":7,"name":"候補","extra":true}]));
                assert_eq!(requests().last().unwrap()["path"], path);
            }
            assert_eq!(
                s.resolve(candidate, Selection::Name("候補"))
                    .await
                    .unwrap()
                    .to_string(),
                "7"
            );
            assert_eq!(
                s.resolve(candidate, Selection::Id("9"))
                    .await
                    .unwrap()
                    .to_string(),
                "9"
            );
        }
        let all = requests();
        assert_eq!(all.len(), 18);
        assert!(all.iter().all(|r| r["method"] == "GET"));
        assert!(all.iter().all(|r| {
            r["headers"]
                .as_object()
                .unwrap()
                .iter()
                .any(|(name, value)| {
                    name.eq_ignore_ascii_case("Backlog-API-Key") && value == "test-secret"
                })
        }));
    });
}

#[test]
fn resolution_zero_is_a_value_not_unset() {
    if library_process(
        "resolution_zero_is_a_value_not_unset",
        json!({"body":r#"[{"id":0,"name":"対応済み"}]"#}),
    ) {
        return;
    }
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let s = service();
        assert_eq!(
            s.resolve(Candidate::Resolution, Selection::Id("0"))
                .await
                .unwrap()
                .to_string(),
            "0"
        );
        assert!(requests().is_empty());
        assert_eq!(
            s.resolve(Candidate::Resolution, Selection::Name("対応済み"))
                .await
                .unwrap()
                .to_string(),
            "0"
        );
        assert_eq!(
            s.candidates(Candidate::Resolution).await.unwrap()[0]["id"],
            0
        );
        let e = s.candidates(Candidate::Priority).await.unwrap_err();
        assert_eq!(e.kind, "invalid_response");
        assert_eq!(e.http_status, Some(200));
        assert_eq!(requests().len(), 3);
    });
}

#[test]
fn project_is_one_safe_path_segment_and_invalid_segments_never_send_http() {
    if library_process(
        "project_is_one_safe_path_segment_and_invalid_segments_never_send_http",
        json!({"body":"[]"}),
    ) {
        return;
    }
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let s = service();
        for project in ["", ".", ".."] {
            let e = s
                .candidates(Candidate::IssueType { project })
                .await
                .unwrap_err();
            assert_eq!((e.kind, e.exit_code()), ("input", 2));
        }
        assert!(requests().is_empty());
        s.candidates(Candidate::IssueType {
            project: "a/b%?#!\\ 日本",
        })
        .await
        .unwrap();
        assert_eq!(
            requests()[0]["path"],
            "/api/v2/projects/a%2Fb%25%3F%23!%5C%20%E6%97%A5%E6%9C%AC/issueTypes"
        );
        assert_eq!(requests().len(), 1);
    });
}

#[test]
fn assignee_self_fetches_only_selected_profiles_myself_on_demand() {
    if library_process(
        "assignee_self_fetches_only_selected_profiles_myself_on_demand",
        json!({"body":r#"{"id":123456789012345678901234567890,"name":"自分","extra":true}"#}),
    ) {
        return;
    }
    let origin = std::env::var("SERVICE_TEST").unwrap();
    let root = std::path::PathBuf::from(std::env::var("SERVICE_ROOT").unwrap());
    std::fs::write(
        root.join("backlog/config.json"),
        json!({"profiles":{
            "default":{"spaceUrl":"https://unused.invalid","apiKeyEnv":"UNSET"},
            "work":{"spaceUrl":origin,"apiKeyEnv":"TEST_KEY"}
        }})
        .to_string(),
    )
    .unwrap();
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let s = Service::new(&Config::load("work").unwrap()).unwrap();
        assert_eq!(
            s.resolve(Candidate::Assignee { project: "P" }, Selection::Id("9"))
                .await
                .unwrap()
                .to_string(),
            "9"
        );
        assert!(requests().is_empty());
        assert_eq!(
            s.resolve(
                Candidate::Assignee { project: "P" },
                Selection::Name("self")
            )
            .await
            .unwrap()
            .to_string(),
            "123456789012345678901234567890"
        );
        assert_eq!(s.myself().await.unwrap()["extra"], true);
        let all = requests();
        assert_eq!(all.len(), 2);
        assert!(all.iter().all(|r| r["path"] == "/api/v2/users/myself"));
        assert!(all.iter().all(|r| {
            r["headers"]
                .as_object()
                .unwrap()
                .iter()
                .any(|(name, value)| {
                    name.eq_ignore_ascii_case("Backlog-API-Key") && value == "test-secret"
                })
        }));
    });
}

fn response(value: serde_json::Value) {
    let root = std::path::PathBuf::from(std::env::var("SERVICE_ROOT").unwrap());
    std::fs::write(root.join("response.json"), value.to_string()).unwrap();
}
fn candidates() -> [Candidate<'static>; 6] {
    [
        Candidate::IssueType { project: "P" },
        Candidate::Status { project: "P" },
        Candidate::Assignee { project: "P" },
        Candidate::Milestone { project: "P" },
        Candidate::Priority,
        Candidate::Resolution,
    ]
}

#[test]
fn candidate_names_are_exact_and_missing_or_duplicate_names_are_input_errors() {
    if library_process(
        "candidate_names_are_exact_and_missing_or_duplicate_names_are_input_errors",
        json!({"body":"[]"}),
    ) {
        return;
    }
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let s = service();
        for candidate in candidates() {
            for body in ["[]", r#"[{"id":1,"name":"Same"},{"id":2,"name":"Same"}]"#] {
                response(json!({"body":body}));
                let e = s.resolve(candidate, Selection::Name("Same")).await.unwrap_err();
                assert_eq!((e.kind, e.exit_code()), ("input", 2));
            }
            response(json!({"body":r#"[{"id":8,"name":" Self "},{"id":9,"name":"123"},{"id":10,"name":"self"}]"#}));
            assert_eq!(s.resolve(candidate, Selection::Name("123")).await.unwrap().to_string(), "9");
            for name in ["Self", " SELF ", "１２３"] {
                assert_eq!(s.resolve(candidate, Selection::Name(name)).await.unwrap_err().kind, "input");
            }
            if !matches!(candidate, Candidate::Assignee {..}) {
                assert_eq!(s.resolve(candidate, Selection::Name("self")).await.unwrap().to_string(), "10");
            }
        }
    });
}

#[test]
fn candidate_and_myself_failures_propagate_without_empty_or_input_fallback() {
    if library_process(
        "candidate_and_myself_failures_propagate_without_empty_or_input_fallback",
        json!({"body":"[]"}),
    ) {
        return;
    }
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let s = service();
        for (fixture, kind, status) in [
            (
                json!({"status":401,"body":r#"{"errors":[{"code":11,"message":"denied"}]}"#}),
                "auth",
                Some(401),
            ),
            (json!({"status":403}), "permission", Some(403)),
            (json!({"status":404}), "not_found", Some(404)),
            (json!({"status":429}), "rate_limit", Some(429)),
            (json!({"status":500}), "api", Some(500)),
            (
                json!({"status":302,"location":"https://unused.invalid"}),
                "api",
                Some(302),
            ),
            (json!({"disconnect":true}), "network", None),
            (json!({"body":"not JSON"}), "invalid_response", Some(200)),
            (json!({"body":"{}"}), "invalid_response", Some(200)),
        ] {
            response(fixture);
            let before = requests().len();
            for candidate in candidates() {
                let e = s
                    .resolve(candidate, Selection::Name("候補"))
                    .await
                    .unwrap_err();
                assert_eq!((e.kind, e.exit_code(), e.http_status), (kind, 1, status));
                if status == Some(401) {
                    assert_eq!(e.api_errors[0]["code"], 11);
                }
            }
            let e = s
                .resolve(
                    Candidate::Assignee { project: "P" },
                    Selection::Name("self"),
                )
                .await
                .unwrap_err();
            assert_eq!((e.kind, e.exit_code(), e.http_status), (kind, 1, status));
            assert_eq!(requests().len() - before, 7, "no retry or redirect");
        }
        for body in [
            "[]",
            "null",
            r#"{"id":0}"#,
            r#"{"id":"1"}"#,
            r#"{"id":1.5}"#,
        ] {
            response(json!({"body":body}));
            let e = s.myself().await.unwrap_err();
            assert_eq!((e.kind, e.http_status), ("invalid_response", Some(200)));
        }
        for body in [
            "null",
            r#"[null]"#,
            r#"[{"id":"1","name":"N"}]"#,
            r#"[{"id":-1,"name":"N"}]"#,
            r#"[{"id":1.0,"name":"N"}]"#,
            r#"[{"id":1e2,"name":"N"}]"#,
            r#"[{"id":1}]"#,
            r#"[{"id":1,"name":null}]"#,
        ] {
            response(json!({"body":body}));
            for candidate in candidates() {
                let e = s.candidates(candidate).await.unwrap_err();
                assert_eq!((e.kind, e.http_status), ("invalid_response", Some(200)));
            }
        }
    });
}

#[test]
fn shared_search_applies_defaults_preserves_raw_json_and_validates_inputs() {
    use backlog_cli::service::IssueSearch;
    if library_process(
        "shared_search_applies_defaults_preserves_raw_json_and_validates_inputs",
        json!({"routes":{
            "/api/v2/projects/P":{"body":r#"{"id":123456789012345678901234567890}"#},
            "/api/v2/projects/P/statuses":{"body":r#"[{"id":2,"name":"open"}]"#},
            "/api/v2/issues/count":{"body":r#"{"count":123456789012345678901234567890}"#},
            "/api/v2/issues":{"body":r#"[{"id":1,"issueKey":"P-1","summary":"test-secret\u001b[31m","status":{"name":"open"},"extra":123456789012345678901234567890}]"#}
        }}),
    ) {
        return;
    }
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let s = service();
        let settings: Settings = serde_json::from_value(json!({"project":"P","issue":{"list":{"status":["open"]},"create":{"assignee":"never"}}})).unwrap();
        let page = s.search(&settings, IssueSearch::default()).await.unwrap();
        assert_eq!(page["pagination"].to_string(), r#"{"count":1,"limit":20,"offset":0,"total":123456789012345678901234567890}"#);
        assert_eq!(page["items"][0]["summary"], "test-secret\u{1b}[31m");
        assert_eq!(page["items"][0]["extra"].to_string(), "123456789012345678901234567890");
        assert_eq!(requests().len(), 4);
        for search in [
            IssueSearch { status: vec![Selection::Name("open"), Selection::Id("2")], ..Default::default() },
            IssueSearch { assignee: vec![Selection::Name("self"), Selection::Id("2")], ..Default::default() },
            IssueSearch { milestone: vec![Selection::Name("v1"), Selection::Id("2")], ..Default::default() },
            IssueSearch { mine: true, assignee: vec![Selection::Name("self")], ..Default::default() },
            IssueSearch { project: Some("all"), status: vec![Selection::Id("2")], keyword: Some("term"), ..Default::default() },
            IssueSearch { limit: 0, ..Default::default() },
            IssueSearch { offset: "-1", ..Default::default() },
        ] {
            let error = s.search(&settings, search).await.unwrap_err();
            assert_eq!((error.kind, error.exit_code()), ("input", 2));
        }
        assert_eq!(requests().len(), 4);
    });
}

#[test]
fn issue_view_preserves_raw_object_and_project_rule_is_separate_and_validated() {
    let original = json!({
        "id": "123456789012345678901234567890".parse::<serde_json::Number>().unwrap(),
        "projectId": "987654321098765432109876543210".parse::<serde_json::Number>().unwrap(),
        "issueKey":"P-2", "summary":"test-secret\u{1b}[31m", "description":" original\n\t<&> test-secret\u{1b}",
        "issueType":{"name":"Task"}, "priority":{"name":"中"}, "status":{"name":"完了"},
        "assignee":null, "resolution":null, "startDate":null, "dueDate":null,
        "milestone":[], "created":"created", "updated":"updated", "test-secret-field":[true,null]
    });
    if library_process(
        "issue_view_preserves_raw_object_and_project_rule_is_separate_and_validated",
        json!({"body":original.to_string()}),
    ) {
        return;
    }
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let s = service();
        let mut expected = original;
        expected["url"] = json!(format!(
            "{}/view/P-2",
            std::env::var("SERVICE_TEST").unwrap()
        ));
        let value = s.issue_view("0007").await.unwrap();
        assert_eq!(value, expected);
        assert_eq!(requests().len(), 1);
        assert_eq!(requests()[0]["path"], "/api/v2/issues/0007");
        let safe = backlog_cli::safety::json(&value, &["test-secret"]).unwrap();
        assert!(!safe.to_string().contains("test-secret"));
        assert_eq!(value, expected);
        let id = value["projectId"].as_number().unwrap();
        for rule in ["markdown", "backlog", "unknown", ""] {
            response(json!({"body":json!({"textFormattingRule":rule}).to_string()}));
            assert_eq!(s.project_text_formatting_rule(id).await.unwrap(), rule);
        }
        for body in [
            "{}",
            "[]",
            "null",
            "{\"textFormattingRule\":null}",
            "{\"textFormattingRule\":3}",
            "invalid",
        ] {
            response(json!({"body":body}));
            let error = s.project_text_formatting_rule(id).await.unwrap_err();
            assert_eq!(
                (error.kind, error.http_status, error.exit_code()),
                ("invalid_response", Some(200), 1)
            );
        }
        for (status, kind) in [
            (401, "auth"),
            (403, "permission"),
            (404, "not_found"),
            (429, "rate_limit"),
            (500, "api"),
        ] {
            response(json!({"status":status}));
            let error = s.project_text_formatting_rule(id).await.unwrap_err();
            assert_eq!(
                (error.kind, error.http_status, error.exit_code()),
                (kind, Some(status), 1)
            );
        }
        response(json!({"disconnect":true}));
        let error = s.project_text_formatting_rule(id).await.unwrap_err();
        assert_eq!(
            (error.kind, error.http_status, error.exit_code()),
            ("network", None, 1)
        );
        let all = requests();
        assert_eq!(all.len(), 17);
        assert!(
            all[1..]
                .iter()
                .all(|r| r["path"] == "/api/v2/projects/987654321098765432109876543210")
        );
        for invalid in ["0", "-1", "1.5"] {
            let error = s
                .project_text_formatting_rule(&invalid.parse().unwrap())
                .await
                .unwrap_err();
            assert_eq!(
                (error.kind, error.http_status, error.exit_code()),
                ("input", None, 2)
            );
        }
        assert_eq!(requests().len(), 17);
    });
}

#[test]
fn comment_reads_preserve_raw_json_and_validate_inputs_before_requests() {
    let original: serde_json::Value = serde_json::from_str(r##"{"id":123456789012345678901234567890,"content":"# 原文 test-secret\u001b\n\t","createdUser":{"name":"投稿者"},"created":"created","updated":"updated","changeLog":[{"field":"future","originalValue":{"test-secret":[null,123456789012345678901234567890]},"newValue":"test-secret\u001b"}],"unknown":{"large":123456789012345678901234567890}}"##).unwrap();
    if library_process(
        "comment_reads_preserve_raw_json_and_validate_inputs_before_requests",
        json!({"routes":{
            "/api/v2/issues/P-1/comments":{"body":json!([original.clone()]).to_string()},
            "/api/v2/issues/P-1/comments/0007":{"body":original.to_string()}
        }}),
    ) {
        return;
    }
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let s = service();
        for issue in ["", "0", "P-0", "../P-1"] {
            assert_eq!(s.comment_list(issue,20,None,None).await.unwrap_err().kind,"input");
            assert_eq!(s.comment_view(issue,"7").await.unwrap_err().kind,"input");
        }
        for id in ["", "0", "-1", "1e2"] {
            assert_eq!(s.comment_list("P-1",20,Some(id),None).await.unwrap_err().kind,"input");
            assert_eq!(s.comment_list("P-1",20,None,Some(id)).await.unwrap_err().kind,"input");
            assert_eq!(s.comment_view("P-1",id).await.unwrap_err().kind,"input");
        }
        for limit in [0,101] { assert_eq!(s.comment_list("P-1",limit,None,None).await.unwrap_err().kind,"input"); }
        for (min,max) in [("0002","2"),("10","9"),("123456789012345678901234567891","123456789012345678901234567890")] {
            assert_eq!(s.comment_list("P-1",20,Some(min),Some(max)).await.unwrap_err().kind,"input");
        }
        assert!(requests().is_empty());
        assert_eq!(s.comment_view("P-1","0007").await.unwrap(),original);
        assert_eq!(s.comment_list("P-1",20,Some("1"),Some("999999999999999999999999999999")).await.unwrap(),json!({"items":[original]}));
        assert_eq!(requests().len(),2);
        assert_eq!(requests()[1]["path"],"/api/v2/issues/P-1/comments?count=20&order=desc&minId=1&maxId=999999999999999999999999999999");
    });
}

#[test]
fn comment_formatting_uses_only_target_issue_project_and_keeps_api_failures() {
    if library_process(
        "comment_formatting_uses_only_target_issue_project_and_keeps_api_failures",
        json!({"routes":{
            "/api/v2/issues/P-1":{"body":"{\"projectId\":987654321098765432109876543210,\"unknown\":true}"},
            "/api/v2/projects/987654321098765432109876543210":{"body":"{\"textFormattingRule\":\"future-rule\"}"}
        }}),
    ) {
        return;
    }
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let s = service();
        assert_eq!(
            s.issue_text_formatting_rule("P-1").await.unwrap(),
            "future-rule"
        );
        assert_eq!(requests().len(), 2);
        assert_eq!(requests()[0]["path"], "/api/v2/issues/P-1");
        assert_eq!(
            requests()[1]["path"],
            "/api/v2/projects/987654321098765432109876543210"
        );
        let response_path =
            std::path::Path::new(&std::env::var("SERVICE_ROOT").unwrap()).join("response.json");
        for bad in [
            json!({}),
            json!({"projectId":null}),
            json!({"projectId":0}),
            json!({"projectId":1.5}),
            json!({"projectId":"1"}),
        ] {
            std::fs::write(&response_path, json!({"body":bad.to_string()}).to_string()).unwrap();
            let error = s.issue_text_formatting_rule("P-1").await.unwrap_err();
            assert_eq!(error.kind, "invalid_response");
            assert_eq!(error.http_status, Some(200));
        }
        std::fs::write(&response_path, json!({"status":403}).to_string()).unwrap();
        for error in [
            s.comment_list("P-1", 20, None, None).await.unwrap_err(),
            s.comment_view("P-1", "7").await.unwrap_err(),
            s.issue_text_formatting_rule("P-1").await.unwrap_err(),
        ] {
            assert_eq!(error.kind, "permission");
            assert_eq!(error.http_status, Some(403));
            assert_eq!(error.exit_code(), 1);
        }
    });
}
