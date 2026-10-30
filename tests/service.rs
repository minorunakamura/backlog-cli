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

#[test]
fn issue_create_preserves_raw_response_and_body_and_validates_before_post() {
    use backlog_cli::service::IssueCreate;
    let original: serde_json::Value=serde_json::from_str(r#"{"id":123456789012345678901234567890,"issueKey":"P-1","test-secret":{"body":"  test-secret\u001b\n"},"url":"https://untrusted.invalid"}"#).unwrap();
    if library_process(
        "issue_create_preserves_raw_response_and_body_and_validates_before_post",
        json!({"status":201,"body":original.to_string()}),
    ) {
        return;
    }
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let s=service();
        let defaults: Settings=serde_json::from_value(json!({"project":"123456789012345678901234567890","issue":{"create":{"issueType":"unused","priority":"unused","assignee":"unused"}}})).unwrap();
        let make=||IssueCreate {title:" 原文 test-secret\u{1b} ",body:Some(" \n# **test-secret**\u{1b}\t\n "),issue_type:Some(Selection::Id("0007")),priority:Some(Selection::Id("3")),clear_assignee:true,..Default::default()};
        let mut expected=original;
        expected["url"]=json!(format!("{}/view/P-1",std::env::var("SERVICE_TEST").unwrap()));
        let value=s.issue_create(&defaults,make()).await.unwrap();
        assert_eq!(value,expected);
        assert_eq!(requests().len(),1);
        assert_eq!(requests()[0]["form"],json!({"projectId":["123456789012345678901234567890"],"issueTypeId":["7"],"priorityId":["3"],"summary":[" 原文 test-secret\u{1b} "],"description":[" \n# **test-secret**\u{1b}\t\n "]}));
        for create in [
            IssueCreate{title:" \n\t",..make()},
            IssueCreate{assignee:Some(Selection::Name("self")),..make()},
            IssueCreate{start_date:Some("2100-02-29"),..make()},
            IssueCreate{due_date:Some("2024-04-31"),..make()},
            IssueCreate{milestone:vec![Selection::Name("v1"),Selection::Id("1")],..make()},
            IssueCreate{issue_type:Some(Selection::Id("0")),..make()},
            IssueCreate{priority:Some(Selection::Id("1e2")),..make()},
            IssueCreate{milestone:vec![Selection::Id("0")],..make()},
            IssueCreate{project:Some("all"),..make()},
        ] {
            let e=s.issue_create(&defaults,create).await.unwrap_err();
            assert_eq!((e.kind,e.exit_code(),e.http_status),("input",2,None));
            assert_eq!(serde_json::to_value(e).unwrap()["write"],json!({"operation":"issue.create","outcome":"not_created"}));
        }
        assert_eq!(requests().len(),1);
        // Gregorian leap-century dates are valid, without enforcing date ordering.
        let _=s.issue_create(&defaults,IssueCreate{start_date:Some("2400-02-29"),due_date:Some("2000-02-29"),..make()}).await.unwrap();
        assert_eq!(requests().len(),2);
        response(json!({"status":201,"body":"not json"}));
        let e=s.issue_create(&defaults,make()).await.unwrap_err();
        assert_eq!((e.kind,e.http_status),("invalid_response",Some(201)));
        assert_eq!(serde_json::to_value(e).unwrap()["write"]["outcome"],"created");
        response(json!({"status":500}));
        let e=s.issue_create(&defaults,IssueCreate{priority:Some(Selection::Name("中")),..make()}).await.unwrap_err();
        assert_eq!((e.kind,e.http_status),("api",Some(500)));
        assert_eq!(serde_json::to_value(e).unwrap()["write"]["outcome"],"not_created");
        assert_eq!(requests().last().unwrap()["method"],"GET");
    });
}

#[test]
fn issue_update_compares_latest_title_and_preserves_raw_response() {
    use backlog_cli::service::IssueUpdate;
    let original = json!({"id":1,"issueKey":"P-1","summary":"changed test-secret\u{1b}","unknown":["123456789012345678901234567890".parse::<serde_json::Number>().unwrap()]});
    if library_process(
        "issue_update_compares_latest_title_and_preserves_raw_response",
        json!({"routes":{
            "GET /api/v2/issues/P-1":{"body":"{\"projectId\":42,\"summary\":\"old\"}"},
            "PATCH /api/v2/issues/P-1":{"body":original.to_string()}
        }}),
    ) {
        return;
    }
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let s = service();
        let value = s
            .issue_update(
                "P-1",
                IssueUpdate {
                    title: Some("changed test-secret\u{1b}"),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        let mut expected = original;
        expected["url"] = json!(format!(
            "{}/view/P-1",
            std::env::var("SERVICE_TEST").unwrap()
        ));
        assert_eq!(value, expected);
        assert_eq!(requests().len(), 2);
        assert_eq!(requests()[1]["method"], "PATCH");
        assert_eq!(
            requests()[1]["form"],
            json!({"summary":["changed test-secret\u{1b}"]})
        );
        let error = s
            .issue_update(
                "P-1",
                IssueUpdate {
                    title: Some("old"),
                    ..Default::default()
                },
            )
            .await
            .unwrap_err();
        assert_eq!((error.kind, error.exit_code()), ("input", 2));
        assert_eq!(
            serde_json::to_value(error).unwrap()["write"],
            json!({"operation":"issue.update","outcome":"not_updated"})
        );
        assert_eq!(requests().len(), 3);
    });
}

#[test]
fn issue_update_validates_public_inputs_and_only_used_comparison_fields() {
    use backlog_cli::service::IssueUpdate;
    if library_process(
        "issue_update_validates_public_inputs_and_only_used_comparison_fields",
        json!({"routes":{
            "GET /api/v2/issues/0007":{"body":"{\"projectId\":987654321098765432109876543210,\"description\":null,\"assignee\":null}"},
            "PATCH /api/v2/issues/0007":{"body":"{\"id\":7,\"issueKey\":\"P-7\",\"raw\":\"test-secret\\u001b\"}"}
        }}),
    ) {
        return;
    }
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let s=service();
        for update in [
            IssueUpdate{title:Some(" \n\t"),..Default::default()},
            IssueUpdate{assignee:Some(Selection::Name("self")),clear_assignee:true,..Default::default()},
            IssueUpdate{resolution:Some(Selection::Id("0")),clear_resolution:true,..Default::default()},
            IssueUpdate{start_date:Some("2024-01-01"),clear_start_date:true,..Default::default()},
            IssueUpdate{due_date:Some("2024-01-01"),clear_due_date:true,..Default::default()},
            IssueUpdate{milestone:vec![Selection::Id("1")],clear_milestones:true,..Default::default()},
            IssueUpdate{milestone:vec![Selection::Name("v1"),Selection::Id("1")],..Default::default()},
            IssueUpdate{issue_type:Some(Selection::Id("0")),..Default::default()},
            IssueUpdate{priority:Some(Selection::Id("1e2")),..Default::default()},
            IssueUpdate{status:Some(Selection::Id("-1")),..Default::default()},
            IssueUpdate{assignee:Some(Selection::Id("0")),..Default::default()},
            IssueUpdate{resolution:Some(Selection::Id("1.5")),..Default::default()},
            IssueUpdate{milestone:vec![Selection::Id("0")],..Default::default()},
            IssueUpdate{start_date:Some("2100-02-29"),..Default::default()},
            IssueUpdate{due_date:Some("2024-04-31"),..Default::default()},
        ] {
            let e=s.issue_update("0007",update).await.unwrap_err();
            assert_eq!((e.kind,e.exit_code(),e.http_status),("input",2,None));
            assert_eq!(serde_json::to_value(e).unwrap()["write"],json!({"operation":"issue.update","outcome":"not_updated"}));
        }
        for issue in ["","0","P-0","../P-1"] {assert_eq!(s.issue_update(issue,IssueUpdate::default()).await.unwrap_err().kind,"input");}
        assert!(requests().is_empty());
        let value=s.issue_update("0007",IssueUpdate{body:Some(" \n原文 test-secret\u{1b}\t "),assignee:Some(Selection::Id("00123456789012345678901234567890")),..Default::default()}).await.unwrap();
        assert_eq!(value["raw"],"test-secret\u{1b}");
        assert_eq!(value["url"],format!("{}/view/P-7",std::env::var("SERVICE_TEST").unwrap()));
        assert_eq!(requests().len(),2);
        assert_eq!(requests()[0]["path"],"/api/v2/issues/0007");
        assert_eq!(requests()[1]["form"],json!({"description":[" \n原文 test-secret\u{1b}\t "],"assigneeId":["123456789012345678901234567890"]}));
        // The minimal latest object has no display names or omitted fields.
        for update in [IssueUpdate{body:Some(""),..Default::default()},IssueUpdate{clear_assignee:true,..Default::default()}] {
            assert_eq!(s.issue_update("0007",update).await.unwrap_err().kind,"input");
        }
        assert_eq!(requests().len(),4);
        for body in [
            json!({"projectId":42,"status":{"name":"open"}}),
            json!({"projectId":42,"status":{"id":0}}),
            json!({"projectId":42,"status":{"id":"1"}}),
            json!({"projectId":42,"status":null}),
        ] {
            response(json!({"body":body.to_string()}));
            let e=s.issue_update("0007",IssueUpdate{status:Some(Selection::Id("2")),..Default::default()}).await.unwrap_err();
            assert_eq!((e.kind,e.http_status),("invalid_response",Some(200)));
            assert_eq!(serde_json::to_value(e).unwrap()["write"]["outcome"],"not_updated");
        }
        assert_eq!(requests().len(),8);
        response(json!({"routes":{
            "GET /api/v2/issues/0007":{"body":"{\"projectId\":42,\"startDate\":null,\"dueDate\":null}"},
            "PATCH /api/v2/issues/0007":{"body":"not json"}
        }}));
        let e=s.issue_update("0007",IssueUpdate{start_date:Some("2400-02-29"),due_date:Some("2000-02-29"),..Default::default()}).await.unwrap_err();
        assert_eq!((e.kind,e.http_status),("invalid_response",Some(200)));
        assert_eq!(serde_json::to_value(e).unwrap()["write"]["outcome"],"updated");
        assert_eq!(requests().last().unwrap()["form"],json!({"startDate":["2400-02-29"],"dueDate":["2000-02-29"]}));
    });
}

#[test]
fn comment_add_preserves_raw_data_and_reuses_partial_update_validation() {
    use backlog_cli::service::CommentAdd;
    if library_process(
        "comment_add_preserves_raw_data_and_reuses_partial_update_validation",
        json!({"routes":{
            "GET /api/v2/issues/1":{"body":r#"{"projectId":42,"resolution":{"id":0}}"#},
            "POST /api/v2/issues/1/comments":{"status":201,"body":r#"{"id":123456789012345678901234567890,"content":" 原文 test-secret\n","unknown":123456789012345678901234567890}"#},
            "PATCH /api/v2/issues/1":{"body":r#"{"id":1,"issueKey":"P-1","unknown":"test-secret"}"#}
        }}),
    ) {
        return;
    }
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let s = service();
        for add in [
            CommentAdd::default(),
            CommentAdd {
                status: Some(Selection::Id("0")),
                ..Default::default()
            },
            CommentAdd {
                assignee: Some(Selection::Id("0")),
                ..Default::default()
            },
            CommentAdd {
                resolution: Some(Selection::Id("-1")),
                ..Default::default()
            },
            CommentAdd {
                assignee: Some(Selection::Id("1")),
                clear_assignee: true,
                ..Default::default()
            },
            CommentAdd {
                resolution: Some(Selection::Id("0")),
                clear_resolution: true,
                ..Default::default()
            },
        ] {
            let error = s.comment_add("1", add).await.unwrap_err();
            assert_eq!(error.kind, "input");
            assert_eq!(
                serde_json::to_value(error.write).unwrap(),
                json!({"operation":"comment.add","outcome":"not_created"})
            );
        }
        assert!(requests().is_empty());
        let value = s
            .comment_add(
                "1",
                CommentAdd {
                    body: Some(" 原文 test-secret\n"),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(
            value["data"]["id"].to_string(),
            "123456789012345678901234567890"
        );
        assert_eq!(
            value["data"]["unknown"].to_string(),
            "123456789012345678901234567890"
        );
        assert_eq!(
            value["data"]["content"], " 原文 test-secret\n",
            "library preserves raw data; CLI safety is separate"
        );
        let value = s
            .comment_add(
                "1",
                CommentAdd {
                    body: Some(""),
                    clear_resolution: true,
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(
            value,
            json!({"resource":"issue","data":{"id":1,"issueKey":"P-1","unknown":"test-secret"}})
        );
        let r = requests();
        assert_eq!(r.len(), 3);
        assert_eq!(r[0]["form"], json!({"content":[" 原文 test-secret\n"]}));
        assert_eq!(r[1]["method"], "GET");
        assert_eq!(r[2]["form"], json!({"resolutionId":[""]}));
    });
}

#[test]
fn comment_update_validates_input_and_preserves_raw_object_without_extra_fields() {
    const LARGE: &str = "123456789012345678901234567890";
    let path = format!("/api/v2/issues/{LARGE}/comments/{LARGE}");
    let updated = r#"{"id":123456789012345678901234567890,"content":" 原文 test-secret\n","createdUser":{"name":"author"},"created":"created","updated":"updated","changeLog":null,"unknown":123456789012345678901234567890,"url":"original"}"#;
    if library_process(
        "comment_update_validates_input_and_preserves_raw_object_without_extra_fields",
        json!({"routes":{
            format!("GET {path}"):{"body":r#"{"id":123456789012345678901234567890,"content":"old","createdUser":{"id":123456789012345678901234567890,"name":"author"},"created":"created","updated":"updated","changeLog":null}"#},
            "GET /api/v2/users/myself":{"body":r#"{"id":123456789012345678901234567890}"#},
            format!("PATCH {path}"):{"body":updated}
        }}),
    ) {
        return;
    }
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let s = service();
        for (issue, comment, body) in [
            (LARGE, LARGE, ""),
            ("0", LARGE, "new"),
            ("../P-1", LARGE, "new"),
            (LARGE, "0", "new"),
            (LARGE, "1.5", "new"),
        ] {
            let error = s.comment_update(issue, comment, body).await.unwrap_err();
            assert_eq!(error.kind, "input");
            assert_eq!(error.exit_code(), 2);
            assert_eq!(
                serde_json::to_value(error.write).unwrap(),
                json!({"operation":"comment.update","outcome":"not_updated"})
            );
        }
        assert!(requests().is_empty());
        let value = s
            .comment_update(LARGE, LARGE, " 原文 test-secret\n")
            .await
            .unwrap();
        assert_eq!(
            value,
            serde_json::from_str::<serde_json::Value>(updated).unwrap()
        );
        assert_eq!(value["id"].to_string(), LARGE);
        assert_eq!(value["unknown"].to_string(), LARGE);
        assert_eq!(
            value["content"], " 原文 test-secret\n",
            "library keeps raw secrets and body"
        );
        assert_eq!(value["url"], "original");
        assert!(
            value["createdUser"].get("id").is_none(),
            "no extra post-write ownership requirement"
        );
        let r = requests();
        assert_eq!(r.len(), 3);
        assert_eq!(r[0]["path"], path);
        assert_eq!(r[2]["path"], path);
        assert_eq!(r[2]["method"], "PATCH");
        assert_eq!(r[2]["form"], json!({"content":[" 原文 test-secret\n"]}));
    });
}

#[test]
fn resolved_search_pages_reuse_conditions_without_count_or_name_resolution() {
    use backlog_cli::service::IssueSearch;
    if library_process(
        "resolved_search_pages_reuse_conditions_without_count_or_name_resolution",
        json!({"routes":{
            "/api/v2/projects/P":{"body":"{\"id\":123456789012345678901234567890}"},
            "/api/v2/projects/P/statuses":{"body":"[{\"id\":2,\"name\":\"open\"}]"},
            "/api/v2/issues/count":{"body":"{\"count\":123456789012345678901234567890}"},
            "/api/v2/issues":{"body":"[{\"id\":1,\"issueKey\":\"P-1\",\"summary\":\"raw\",\"status\":{\"name\":\"open\"},\"unknown\":123456789012345678901234567890}]"}
        }}),
    ) {
        return;
    }
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let s = service();
        let settings: Settings = serde_json::from_value(json!({"project":"P","issue":{"list":{"status":["open"]}}})).unwrap();
        let resolved = s.resolve_search(&settings, IssueSearch::default()).await.unwrap();
        let first = s.search_resolved(&resolved).await.unwrap();
        let items = s.search_page(&resolved, "1").await.unwrap();
        assert_eq!(items, first["items"]);
        assert_eq!(first["pagination"]["total"].to_string(), "123456789012345678901234567890");
        let requests = requests();
        assert_eq!(requests.len(), 5);
        let path = requests[4]["path"].as_str().unwrap();
        for part in ["projectId%5B%5D=123456789012345678901234567890", "statusId%5B%5D=2", "sort=updated", "order=desc", "count=20", "offset=1"] { assert!(path.contains(part), "{path}"); }
        assert!(s.search_page(&resolved, "-1").await.is_err());
        response(json!({"routes": {"/api/v2/issues/count":{"status":500}, "/api/v2/issues":{"body":"[]"}}}));
        assert_eq!(s.search_resolved(&resolved).await.unwrap_err().kind, "api");
        assert_eq!(self::requests().len(), 6, "count failure must not become an empty page");
    });
}

#[test]
fn comment_page_supports_ascending_and_descending_without_changing_cli_list_order() {
    use backlog_cli::service::CommentOrder;
    let item = |id| json!({"id":id,"content":null,"changeLog":[],"createdUser":{"name":"Author"},"created":"date","updated":"date"});
    if library_process(
        "comment_page_supports_ascending_and_descending_without_changing_cli_list_order",
        json!({"routes":{
            "/api/v2/issues/P-1/comments":{"commentItems":[item(1),item(2),item(3)]}
        }}),
    ) {
        return;
    }
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let s = service();
        for order in [CommentOrder::Asc, CommentOrder::Desc] {
            for (limit, min, max) in [
                (0, None, None),
                (101, None, None),
                (20, Some("0"), None),
                (20, None, Some("-1")),
                (20, Some("2"), Some("2")),
            ] {
                assert_eq!(
                    s.comment_page("P-1", limit, min, max, order)
                        .await
                        .unwrap_err()
                        .kind,
                    "input"
                );
            }
        }
        assert!(requests().is_empty());
        assert_eq!(
            s.comment_page("P-1", 1, Some("1"), None, CommentOrder::Asc)
                .await
                .unwrap(),
            json!({"items":[item(2)]})
        );
        assert_eq!(
            s.comment_page("P-1", 1, None, Some("3"), CommentOrder::Desc)
                .await
                .unwrap(),
            json!({"items":[item(2)]})
        );
        assert_eq!(
            s.comment_list("P-1", 1, None, None).await.unwrap(),
            json!({"items":[item(3)]})
        );
        let paths: Vec<_> = requests()
            .iter()
            .map(|r| r["path"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(
            paths,
            [
                "/api/v2/issues/P-1/comments?count=1&order=asc&minId=1",
                "/api/v2/issues/P-1/comments?count=1&order=desc&maxId=3",
                "/api/v2/issues/P-1/comments?count=1&order=desc"
            ]
        );
    });
}

#[test]
fn create_task_abort_joins_without_detached_http_or_later_certainty_changes() {
    use backlog_cli::{CreateProgress, WriteOutcome, service::IssueCreate};
    if std::env::var_os("SERVICE_TEST").is_none() {
        for phase in ["before", "sent", "headers"] {
            let h = Harness::new(json!({"routes": {
                "/api/v2/projects/1/issueTypes":{"header_delay":0.5,"body":"[{\"id\":2,\"name\":\"Task\"}]"},
                "POST /api/v2/issues":{"status":201,"header_delay":if phase == "sent" {0.5} else {0.0},"delay":0.5,"body":"{\"id\":9,\"issueKey\":\"A-9\"}"}
            }}));
            let template = h.command(&[]);
            let output = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "create_task_abort_joins_without_detached_http_or_later_certainty_changes",
                    "--nocapture",
                ])
                .env_clear()
                .envs(template.get_envs().filter_map(|(k, v)| v.map(|v| (k, v))))
                .env("SERVICE_TEST", &h.origin)
                .env("SERVICE_ROOT", &h.root)
                .env("SERVICE_PHASE", phase)
                .current_dir(&h.root)
                .output()
                .unwrap();
            assert!(output.status.success(), "{phase}: {output:?}");
            assert_eq!(
                h.requests()
                    .iter()
                    .filter(|r| r["method"] == "POST")
                    .count(),
                usize::from(phase != "before")
            );
        }
        return;
    }
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let phase = std::env::var("SERVICE_PHASE").unwrap();
        let config = Config {
            origin: std::env::var("SERVICE_TEST").unwrap().parse().unwrap(),
            api_key: std::env::var("TEST_KEY").unwrap(),
            editor: vec!["vim".into()],
            settings: Settings::default(),
        };
        let progress = CreateProgress::default();
        let service = Service::with_create_progress(&config, progress.clone()).unwrap();
        let before = phase == "before";
        let task = tokio::spawn(async move {
            service
                .issue_create(
                    &Settings::default(),
                    IssueCreate {
                        project: Some("1"),
                        title: "Raw title",
                        issue_type: Some(if before {
                            Selection::Name("Task")
                        } else {
                            Selection::Id("2")
                        }),
                        priority: Some(Selection::Id("3")),
                        ..Default::default()
                    },
                )
                .await
        });
        let marker = std::path::PathBuf::from(std::env::var("SERVICE_ROOT").unwrap()).join(
            match phase.as_str() {
                "before" => "get-received",
                "sent" => "post-received",
                _ => "post-headers-sent",
            },
        );
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            while !marker.exists() {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        let (outcome, status) = progress.snapshot();
        assert!(
            matches!(
                (phase.as_str(), outcome),
                ("before", WriteOutcome::NotCreated)
                    | ("sent", WriteOutcome::Unknown)
                    | ("headers", WriteOutcome::Created)
            ),
            "{phase}: {outcome:?}"
        );
        assert_eq!(status, if phase == "headers" { Some(201) } else { None });
        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
        assert_eq!(
            format!("{:?}", progress.snapshot()),
            format!("{:?}", (outcome, status))
        );
    });
}
