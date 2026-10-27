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
