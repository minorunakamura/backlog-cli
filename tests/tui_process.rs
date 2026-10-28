mod common;
use common::Harness;
use serde_json::{Value, json};
use std::process::Command;

fn shell(h: &Harness, mode: &str, args: &[&str]) -> Value {
    shell_waiting(h, mode, args, "")
}
fn shell_waiting(h: &Harness, mode: &str, args: &[&str], marker: &str) -> Value {
    let configured = h.command(&[]);
    let output = Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/tty_shell.py"
        ))
        .arg(mode)
        .arg(env!("CARGO_BIN_EXE_bl"))
        .args(args)
        .env_clear()
        .envs(configured.get_envs().map(|(k, v)| (k, v.unwrap())))
        .env("SHELL_STEPS_FILE", h.root.join("shell-steps.json"))
        .env("SHELL_REQUESTS_FILE", h.root.join("requests"))
        .env(
            "SHELL_WAIT_MARKER",
            if marker.is_empty() {
                std::path::PathBuf::new()
            } else {
                h.root.join(marker)
            },
        )
        .current_dir(&h.root)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn bl_requires_both_stdin_and_stdout_tty_before_configuration_or_network() {
    let h = Harness::new(json!({"body":"[]"}));
    std::fs::remove_file(h.root.join("backlog/config.json")).unwrap();
    for mode in ["stdin-only", "stdout-only"] {
        let result = shell(&h, mode, &[]);
        assert_eq!(result["code"], 1, "{result}");
        assert!(
            result["stderr"]
                .as_str()
                .unwrap()
                .contains("stdin and stdout"),
            "{result}"
        );
        assert_eq!(result["restored"], true);
        assert!(!result["stdout"].as_str().unwrap().contains("\x1b[?1049h"));
    }
    assert!(h.requests().is_empty());
}

#[test]
fn bl_loads_real_space_and_projects_using_selected_profile_and_restores_on_q() {
    let h = Harness::new(json!({"routes": {
        "/api/v2/space": {"body":"{\"name\":\"Actual space\"}"},
        "/api/v2/projects": {"body":"[{\"id\":1,\"projectKey\":\"REAL\",\"name\":\"Real project\"}]"}
    }}));
    h.config(json!({"profiles":{"work":{"spaceUrl":h.origin,"apiKeyEnv":"TEST_KEY"}}}));
    let result = shell(&h, "loaded", &["--profile", "work"]);
    assert_eq!(result["code"], 0, "{result}");
    assert_eq!(result["stderr"], "", "{result}");
    assert_eq!(result["restored"], true, "{result}");
    let screen = result["screen"].as_str().unwrap();
    for text in [
        "Actual space",
        "REAL",
        "Real project",
        "Workspace",
        "Issue List",
        "Issue Detail",
        "Comment",
        "Not implemented",
    ] {
        assert!(screen.contains(text), "missing {text}: {result}");
    }
    let output = result["stdout"].as_str().unwrap();
    assert!(
        output.contains("\x1b[?1049h") && output.contains("\x1b[?1049l"),
        "{result}"
    );
    assert!(
        output.ends_with("\x1b[?25h"),
        "cursor not restored: {result}"
    );
    let requests = h.requests();
    assert_eq!(requests.len(), 2, "{requests:?}");
    assert_eq!(requests[0]["path"], "/api/v2/space");
    assert_eq!(requests[1]["path"], "/api/v2/projects");
    for request in requests {
        assert_eq!(request["method"], "GET");
        assert!(
            request["headers"]
                .as_object()
                .unwrap()
                .iter()
                .any(|(name, value)| name.eq_ignore_ascii_case("backlog-api-key")
                    && value == "test-secret")
        );
    }
}

#[test]
fn bl_delayed_startup_is_quittable_and_interruptible_without_waiting_for_network() {
    for phase in ["header_delay", "delay"] {
        for (mode, code) in [("quit", 0), ("interrupt", 130), ("signal", 130)] {
            let h = Harness::new(json!({"body":"{\"name\":\"Actual space\"}", (phase): 20}));
            let result = shell_waiting(
                &h,
                mode,
                &[],
                if phase == "delay" {
                    "headers-sent"
                } else {
                    "requests"
                },
            );
            assert_eq!(result["code"], code, "{phase}/{mode}: {result}");
            assert_eq!(result["restored"], true, "{result}");
            assert!(result["elapsed"].as_f64().unwrap() < 5.0, "{result}");
            let output = result["stdout"].as_str().unwrap();
            assert!(
                output.contains("\x1b[?1049l") && output.ends_with("\x1b[?25h"),
                "{result}"
            );
            if code == 0 {
                assert_eq!(result["stderr"], "");
            } else {
                assert!(result["stderr"].as_str().unwrap().contains("interrupted"));
            }
            assert_eq!(h.requests().len(), 1);
        }
    }
}

#[test]
fn bl_reports_initial_failures_explicitly_and_restores_the_terminal() {
    let cases = [
        ("/api/v2/space", json!({"body":"[]"}), "invalid_response"),
        ("/api/v2/space", json!({"body":"{}"}), "invalid_response"),
        (
            "/api/v2/space",
            json!({"body":"{\"name\":1}"}),
            "invalid_response",
        ),
        (
            "/api/v2/space",
            json!({"body":"not json"}),
            "invalid_response",
        ),
        ("/api/v2/space", json!({"status":401}), "auth"),
        ("/api/v2/space", json!({"disconnect":true}), "network"),
        ("/api/v2/projects", json!({"body":"{}"}), "invalid_response"),
        (
            "/api/v2/projects",
            json!({"body":"[{\"id\":0,\"projectKey\":\"X\",\"name\":\"Bad\"}]"}),
            "invalid_response",
        ),
        (
            "/api/v2/projects",
            json!({"body":"[{\"id\":1,\"projectKey\":\"X\"}]"}),
            "invalid_response",
        ),
        ("/api/v2/projects", json!({"status":403}), "permission"),
        ("/api/v2/projects", json!({"status":429}), "rate_limit"),
        ("/api/v2/projects", json!({"status":500}), "api"),
    ];
    for (path, response, kind) in cases {
        let mut routes = json!({
            "/api/v2/space":{"body":"{\"name\":\"Actual space\"}"},
            "/api/v2/projects":{"body":"[]"}
        });
        routes[path] = response;
        let h = Harness::new(json!({"routes":routes}));
        let result = shell(&h, "failure", &[]);
        assert_eq!(result["code"], 1, "{path}: {result}");
        assert!(
            result["stderr"].as_str().unwrap().contains(kind),
            "{path}: {result}"
        );
        assert!(
            result["stderr"]
                .as_str()
                .unwrap()
                .contains(if path.ends_with("space") {
                    "Initial space"
                } else {
                    "Initial projects"
                }),
            "{path}: {result}"
        );
        assert_eq!(result["restored"], true, "{result}");
        let output = result["stdout"].as_str().unwrap();
        assert!(
            output.contains("\x1b[?1049l") && output.ends_with("\x1b[?25h"),
            "{result}"
        );
        assert!(
            !output.contains("No projects"),
            "failure must not become an empty success: {result}"
        );
        assert_eq!(
            h.requests().len(),
            if path.ends_with("space") { 1 } else { 2 }
        );
    }
}

#[test]
fn bl_argument_errors_do_not_echo_external_secrets_or_terminal_controls() {
    let h = Harness::new(json!({"body":"[]"}));
    let result = shell(&h, "stdin-only", &["--test-secret\x1b]52;c;payload\x07"]);
    assert_eq!(result["code"], 2, "{result}");
    let errors = result["stderr"].as_str().unwrap();
    assert!(
        !errors.contains("test-secret") && !errors.contains('\x1b'),
        "{result}"
    );
    assert!(h.requests().is_empty());
}

#[test]
fn bl_displays_empty_projects_honestly_and_sanitizes_all_initial_names_without_colors() {
    for projects in [
        json!([]),
        json!([{"id":1,"projectKey":"test-secret\u{1b}[31m", "name":"日本語 e\u{301} 👩‍💻\u{1b}]52;c;clipboard\u{7}"}]),
    ] {
        let h = Harness::new(json!({"routes": {
            "/api/v2/space": {"body":json!({"name":"Actual space test-secret\u{1b}[31m\u{1b}]0;title\u{7}"}).to_string()},
            "/api/v2/projects": {"body":projects.to_string()}
        }}));
        let result = shell(&h, "loaded", &[]);
        assert_eq!(result["code"], 0, "{result}");
        let output = result["stdout"].as_str().unwrap();
        assert!(
            !output.contains("test-secret") && !output.contains("\x1b]"),
            "{result}"
        );
        assert!(
            !output.contains("\x1b[31m")
                && !output.contains("\x1b[38;")
                && !output.contains("\x1b[48;"),
            "NO_COLOR: {result}"
        );
        assert!(output.contains("[REDACTED]"), "{result}");
        assert_eq!(result["restored"], true, "{result}");
        if projects.as_array().unwrap().is_empty() {
            assert!(
                result["screen"].as_str().unwrap().contains("No projects"),
                "{result}"
            );
        } else {
            for text in ["日", "本", "語", "e\u{301}", "👩‍💻"] {
                assert!(output.contains(text), "{result}");
            }
        }
    }
}

#[test]
fn bl_help_with_profile_bypasses_config_and_missing_profiles_do_not_fallback() {
    let h = Harness::new(json!({"body":"[]"}));
    let missing = shell(&h, "failure", &["--profile", "missing"]);
    assert_eq!(missing["code"], 1, "{missing}");
    assert!(
        missing["stderr"]
            .as_str()
            .unwrap()
            .contains("Selected profile is missing")
    );
    assert!(!missing["stdout"].as_str().unwrap().contains("\x1b[?1049h"));
    std::fs::remove_file(h.root.join("backlog/config.json")).unwrap();
    let help = shell(&h, "stdin-only", &["--profile", "work", "--help"]);
    assert_eq!(help["code"], 0, "{help}");
    assert!(help["stdout"].as_str().unwrap().contains("--profile"));
    assert_eq!(help["stderr"], "");
    assert!(h.requests().is_empty());
}

fn scripted(h: &Harness, steps: Value) -> Value {
    std::fs::write(h.root.join("shell-steps.json"), steps.to_string()).unwrap();
    let result = shell(h, "scripted", &[]);
    assert_eq!(result["code"], 0, "{result}");
    assert_eq!(result["restored"], true, "{result}");
    assert_eq!(result["stderr"], "", "{result}");
    result
}
fn browsing_routes() -> Value {
    json!({"routes": {
        "/api/v2/space":{"body":"{\"name\":\"Actual space\"}"},
        "/api/v2/projects":{"body":json!([
            {"id":123456789012345678901234567890_u128.to_string().parse::<serde_json::Number>().unwrap(),"projectKey":"A","name":"Alpha"},
            {"id":2,"projectKey":"B","name":"Beta"}
        ]).to_string()},
        "/api/v2/issues/count":{"body":"{\"count\":1}"},
        "/api/v2/issues":{"body":"[{\"id\":1,\"issueKey\":\"B-1\",\"summary\":\"Selected project issue\",\"status\":{\"name\":\"open\"}}]"}
    }})
}
#[test]
fn project_candidate_movement_is_local_and_enter_confirms_without_moving_focus() {
    let h = Harness::new(browsing_routes());
    let result = scripted(
        &h,
        json!([
            {"wait":"Select a project", "keys":"j"},
            {"wait":"> B  Beta", "keys":"\r"},
            {"wait":"Selected project issue", "keys":"\r"},
            {"wait":"Selected project issue", "keys":"q"}
        ]),
    );
    assert_eq!(result["request_counts"], json!([2, 2, 4, 4]), "{result}");
    let snapshots = result["snapshots"].as_array().unwrap();
    assert!(snapshots[0].as_str().unwrap().contains("> Project List"));
    assert!(snapshots[2].as_str().unwrap().contains("> Project List"));
    let requests = h.requests();
    assert_eq!(
        requests.len(),
        4,
        "movement and same-project confirmation must not request: {requests:?}"
    );
    assert!(
        requests[2]["path"]
            .as_str()
            .unwrap()
            .contains("projectId%5B%5D=2")
    );
}

fn settings(h: &Harness, project: &str) {
    std::fs::create_dir_all(h.root.join(".backlog")).unwrap();
    std::fs::write(
        h.root.join(".backlog/settings.json"),
        json!({"project":project}).to_string(),
    )
    .unwrap();
}
#[test]
fn startup_browse_keys_preserve_initial_success_and_errors() {
    for path in ["/api/v2/space", "/api/v2/projects"] {
        for cancel in [false, true] {
            for status in [200, 401, 403] {
                let mut routes = browsing_routes();
                routes["routes"][path]["header_delay"] = json!(1.2);
                routes["routes"][path]["status"] = json!(status);
                let h = Harness::new(routes);
                settings(&h, "B");
                let mut steps = vec![json!({
                    "wait":"Loading projects...", "pause":0.2,
                    "keys":if cancel { "\u{1b}" } else { "R" }
                })];
                if cancel {
                    steps.push(json!({"pause":0.4,"keys":"R"}));
                }
                steps.push(json!({"pause":1.4,"keys":"q"}));
                std::fs::write(h.root.join("shell-steps.json"), json!(steps).to_string()).unwrap();
                let result = shell(&h, "scripted", &[]);
                let context = format!("{path}/Esc={cancel}/status={status}: {result}");
                assert_eq!(result["restored"], true, "{context}");
                let initial_requests = if path.ends_with("space") { 1 } else { 2 };
                assert_eq!(result["request_counts"][0], initial_requests, "{context}");
                if cancel {
                    assert_eq!(result["request_counts"][1], initial_requests, "{context}");
                }
                let requests = h.requests();
                if status == 200 {
                    assert_eq!(result["code"], 0, "{context}");
                    assert_eq!(result["stderr"], "", "{context}");
                    let screen = result["screen"].as_str().unwrap();
                    for text in ["Actual space", "Project: B", "Selected project issue"] {
                        assert!(screen.contains(text), "missing {text}: {context}");
                    }
                    assert!(!screen.contains("Loading"), "{context}");
                    assert!(!screen.contains("Cancelled"), "{context}");
                    assert_eq!(requests.len(), 4, "{context}");
                    assert_eq!(requests[0]["path"], "/api/v2/space");
                    assert_eq!(requests[1]["path"], "/api/v2/projects");
                    for request in &requests[2..] {
                        assert!(
                            request["path"]
                                .as_str()
                                .unwrap()
                                .contains("projectId%5B%5D=2"),
                            "{context}"
                        );
                    }
                } else {
                    assert_eq!(result["code"], 1, "{context}");
                    let stderr = result["stderr"].as_str().unwrap();
                    assert!(
                        stderr.contains(if status == 401 { "auth" } else { "permission" }),
                        "{context}"
                    );
                    assert!(
                        stderr.contains(if path.ends_with("space") {
                            "Initial space"
                        } else {
                            "Initial projects"
                        }),
                        "{context}"
                    );
                    assert_eq!(requests.len(), initial_requests, "{context}");
                }
            }
        }
    }
}

fn issue_items(start: usize, count: usize) -> String {
    Value::Array((start..start+count).map(|n| json!({"id":n,"issueKey":format!("B-{n}"),"summary":format!("Issue {n:02}"),"status":{"name":"open"}})).collect()).to_string()
}
#[test]
fn issue_pages_use_actual_lengths_and_revisit_without_requests_preserving_selection_and_scroll() {
    let mut routes = browsing_routes();
    routes["routes"]["/api/v2/issues/count"] = json!({"body":"{\"count\":25}"});
    routes["routes"]["/api/v2/issues"] = json!({"offsets": {
        "0":{"body":issue_items(1,20)}, "20":{"body":issue_items(21,3)}, "23":{"body":"[]"}
    }});
    let h = Harness::new(routes);
    settings(&h, "B");
    let result = scripted(
        &h,
        json!([
            {"wait":"Issue 01", "keys":"l jjjjjjjjjjjjjjjjjjn".replace(' ',"")},
            {"wait":"Issue 21", "keys":"jjp"},
            {"wait":"> B-19", "keys":"n"},
            {"wait":"> B-23", "keys":"n"},
            {"wait":"No issues", "keys":"n"},
            {"wait":"No issues", "keys":"pp"},
            {"wait":"> B-19", "keys":"q"}
        ]),
    );
    assert_eq!(
        result["request_counts"],
        json!([4, 5, 5, 5, 6, 6, 6]),
        "{result}"
    );
    let requests = h.requests();
    assert_eq!(requests.len(), 6, "{requests:?}");
    for (index, offset) in [(3, 0), (4, 20), (5, 23)] {
        let path = requests[index]["path"].as_str().unwrap();
        assert!(
            path.contains(&format!("offset={offset}")) && path.contains("count=20"),
            "{path}"
        );
    }
}

#[test]
fn configured_project_ids_are_matched_without_narrowing_and_missing_projects_need_confirmation() {
    for project in ["B", "0002", "123456789012345678901234567890", "missing"] {
        let h = Harness::new(browsing_routes());
        settings(&h, project);
        if project == "missing" {
            let result = scripted(
                &h,
                json!([
                    {"wait":"Select a project", "keys":"j"},
                    {"wait":"> B  Beta", "keys":"q"}
                ]),
            );
            assert_eq!(result["request_counts"], json!([2, 2]));
        } else {
            let result = scripted(&h, json!([{ "wait":"Selected project issue", "keys":"q" }]));
            assert_eq!(result["request_counts"], json!([4]));
            let path = h.requests()[2]["path"].as_str().unwrap().to_owned();
            let id = if project == "123456789012345678901234567890" {
                project
            } else {
                "2"
            };
            assert!(path.contains(&format!("projectId%5B%5D={id}")), "{path}");
        }
    }
}

#[test]
fn project_refresh_failure_preserves_candidate_scroll_and_success_does_not_rewrite_issue_project() {
    let mut routes = browsing_routes();
    let mut projects: Vec<Value> = serde_json::from_str(
        routes["routes"]["/api/v2/projects"]["body"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    projects.extend(
        (2..12)
            .map(|n| json!({"id":n+1,"projectKey":format!("P{n}"),"name":format!("Project {n}")})),
    );
    routes["routes"]["/api/v2/projects"] = json!({"sequence":[
        {"body":Value::Array(projects).to_string()}, {"status":500},
        {"body":"[{\"id\":7,\"projectKey\":\"ONLY\",\"name\":\"Refreshed projects\"}]"}
    ]});
    let h = Harness::new(routes);
    settings(&h, "B");
    let result = scripted(
        &h,
        json!([
            {"wait":"Selected project issue","keys":"jjjjjjjjjj"},
            {"wait":"> P11  Project 11","keys":"R"},
            {"wait":"Failed:","keys":"R"},
            {"wait":"Refreshed projects","keys":"q"}
        ]),
    );
    let snapshots = result["snapshots"].as_array().unwrap();
    assert!(
        snapshots[2].as_str().unwrap().contains("> P11  Project 11"),
        "{result}"
    );
    for snapshot in snapshots {
        assert!(
            snapshot
                .as_str()
                .unwrap()
                .contains("Selected project issue"),
            "{result}"
        );
    }
    assert_eq!(result["request_counts"], json!([4, 4, 5, 6]));
    for request in h.requests().iter().skip(4) {
        assert_eq!(request["path"], "/api/v2/projects");
    }
}

#[test]
fn issue_refresh_is_atomic_and_success_discards_history_only_after_count_and_list_succeed() {
    let mut routes = browsing_routes();
    routes["routes"]["/api/v2/issues/count"] = json!({"sequence":[{"body":"{\"count\":25}"},{"body":"{\"count\":888}"},{"body":"{\"count\":999}"}]});
    routes["routes"]["/api/v2/issues"] = json!({"sequence":[
        {"body":issue_items(1,20)}, {"body":issue_items(21,3)}, {"status":500},
        {"body":issue_items(50,1)}, {"body":"[]"}
    ]});
    let h = Harness::new(routes);
    settings(&h, "B");
    let result = scripted(
        &h,
        json!([
            {"wait":"Issue 01","keys":"ln"},
            {"wait":"Issue 21","keys":"jjR"},
            {"wait":"Failed:","keys":"pn"},
            {"wait":"> B-23","keys":"R"},
            {"wait":"Issue 50","keys":"p"},
            {"wait":"Issue 50","keys":"n"},
            {"wait":"No issues","keys":"q"}
        ]),
    );
    let snapshots = result["snapshots"].as_array().unwrap();
    assert!(
        snapshots[2].as_str().unwrap().contains("> B-23"),
        "{result}"
    );
    assert!(
        snapshots[2].as_str().unwrap().contains("total 25"),
        "{result}"
    );
    assert!(
        snapshots[4]
            .as_str()
            .unwrap()
            .contains("total 999 | page 1"),
        "{result}"
    );
    assert_eq!(
        result["request_counts"],
        json!([4, 5, 7, 7, 9, 9, 10]),
        "{result}"
    );
    assert!(
        h.requests()[9]["path"]
            .as_str()
            .unwrap()
            .contains("offset=1")
    );
}

#[test]
fn superseded_failures_cannot_clear_new_busy_and_escape_cancels_reads_without_replacing_the_display()
 {
    let mut routes = browsing_routes();
    routes["routes"]["/api/v2/issues"] = json!({"sequence":[
        {"body":issue_items(1,1)}, {"status":500,"header_delay":0.35},
        {"body":issue_items(50,1),"header_delay":0.75},
        {"body":issue_items(99,1),"delay":0.5}
    ]});
    let h = Harness::new(routes);
    settings(&h, "B");
    let result = scripted(
        &h,
        json!([
            {"wait":"Issue 01","keys":"lR"},
            {"wait":"Loading... Esc: cancel","keys":"R"},
            {"wait":"Loading... Esc: cancel","pause":0.45,"keys":""},
            {"wait":"Issue 50","keys":"R"},
            {"wait":"Loading... Esc: cancel","keys":"\u{1b}"},
            {"wait":"Cancelled; display retained","pause":0.7,"keys":"q"}
        ]),
    );
    let snapshots = result["snapshots"].as_array().unwrap();
    assert!(
        snapshots[2]
            .as_str()
            .unwrap()
            .contains("Loading... Esc: cancel"),
        "{result}"
    );
    assert!(
        snapshots[5].as_str().unwrap().contains("Issue 50"),
        "{result}"
    );
    assert!(
        !snapshots[5].as_str().unwrap().contains("Issue 99"),
        "{result}"
    );
    assert_eq!(
        result["request_counts"],
        json!([4, 6, 8, 8, 10, 10]),
        "{result}"
    );
}

#[test]
fn empty_pages_stop_even_with_huge_totals_and_nonempty_pages_stop_at_or_beyond_total() {
    for (count, items, text) in [
        (
            "123456789012345678901234567890",
            "[]".to_owned(),
            "No issues",
        ),
        ("0", issue_items(1, 2), "Issue 01"),
        ("2", issue_items(1, 2), "Issue 01"),
    ] {
        let mut routes = browsing_routes();
        routes["routes"]["/api/v2/issues/count"] = json!({"body":format!("{{\"count\":{count}}}")});
        routes["routes"]["/api/v2/issues"] = json!({"body":items});
        let h = Harness::new(routes);
        settings(&h, "B");
        let result = scripted(
            &h,
            json!([
                {"wait":text,"keys":"lnnn"}, {"wait":text,"keys":"q"}
            ]),
        );
        assert_eq!(result["request_counts"], json!([4, 4]), "{result}");
    }
}

#[test]
fn failed_issue_refresh_keeps_the_scrolled_selection_visible() {
    let mut routes = browsing_routes();
    routes["routes"]["/api/v2/issues/count"] = json!({"body":"{\"count\":25}"});
    routes["routes"]["/api/v2/issues"] =
        json!({"sequence":[{"body":issue_items(1,20)},{"status":500}]});
    let h = Harness::new(routes);
    settings(&h, "B");
    let result = scripted(
        &h,
        json!([
            {"wait":"Issue 01","keys":"ljjjjjjjjjjjjjjjjjjR"},
            {"wait":"Failed:","keys":"q"}
        ]),
    );
    assert!(
        result["snapshots"][1].as_str().unwrap().contains("> B-19"),
        "{result}"
    );
}

#[test]
fn same_project_confirmation_retains_conditions_and_history_but_different_project_success_replaces_them()
 {
    let mut routes = browsing_routes();
    let huge_id = "123456789012345678901234567890";
    routes["routes"]["/api/v2/projects/2/statuses"] =
        json!({"body":"[{\"id\":4,\"name\":\"open\"}]"});
    routes["routes"][format!("/api/v2/projects/{huge_id}/statuses")] =
        json!({"body":"[{\"id\":8,\"name\":\"open\"}]"});
    routes["routes"]["/api/v2/issues/count"] = json!({"body":"{\"count\":25}"});
    routes["routes"]["/api/v2/issues"] = json!({"sequence":[
        {"body":issue_items(1,20)}, {"body":issue_items(21,3)},
        {"body":issue_items(50,1),"delay":0.4}, {"body":issue_items(60,1)}
    ]});
    let h = Harness::new(routes);
    settings(&h, "B");
    std::fs::write(
        h.root.join(".backlog/settings.json"),
        json!({"project":"B","issue":{"list":{"status":["open"]}}}).to_string(),
    )
    .unwrap();
    let result = scripted(
        &h,
        json!([
            {"wait":"Issue 01","keys":"ln"},
            {"wait":"Issue 21","keys":"jjh\r"},
            {"wait":"> B-23","keys":"k\r"},
            {"wait":"Loading... Esc: cancel","keys":"j\r"},
            {"wait":"> B-23","pause":0.5,"keys":"k\r"},
            {"wait":"Issue 60","keys":"lpn"},
            {"wait":"Issue 60","keys":"q"}
        ]),
    );
    let snapshots = result["snapshots"].as_array().unwrap();
    assert!(
        snapshots[4]
            .as_str()
            .unwrap()
            .contains("Project: B | total 25 | page 2"),
        "{result}"
    );
    assert!(
        !snapshots[4].as_str().unwrap().contains("Issue 50"),
        "{result}"
    );
    assert!(
        snapshots[5]
            .as_str()
            .unwrap()
            .contains("Project: A | total 25 | page 1"),
        "{result}"
    );
    // Last n fetches a new page, never the old project's retained page.
    assert_eq!(
        result["request_counts"],
        json!([5, 6, 6, 9, 9, 12, 13]),
        "{result}"
    );
    let requests = h.requests();
    assert!(
        requests[11]["path"]
            .as_str()
            .unwrap()
            .contains("statusId%5B%5D=8")
    );
    assert!(requests[12]["path"].as_str().unwrap().contains("offset=1"));
}

#[test]
fn huge_total_allows_list_only_next_page_and_issue_display_is_sanitized() {
    let mut routes = browsing_routes();
    routes["routes"]["/api/v2/issues/count"] =
        json!({"body":"{\"count\":123456789012345678901234567890}"});
    let items = json!([{"id":123456789012345678901234567890_u128.to_string().parse::<serde_json::Number>().unwrap(),"issueKey":"B-1","summary":"Safe issue test-secret\u{1b}]52;c;clipboard\u{7}","status":{"name":"open"}}]);
    routes["routes"]["/api/v2/issues"] =
        json!({"offsets":{"0":{"body":items.to_string()},"1":{"body":"[]"}}});
    let h = Harness::new(routes);
    settings(&h, "B");
    let result = scripted(
        &h,
        json!([
            {"wait":"Safe issue","keys":"ln"}, {"wait":"No issues","keys":"nq"}
        ]),
    );
    let output = result["stdout"].as_str().unwrap();
    assert!(
        !output.contains("test-secret") && !output.contains("\u{1b}]52"),
        "{result}"
    );
    assert!(output.contains("[REDACTED]"), "{result}");
    assert_eq!(result["request_counts"], json!([4, 5]), "{result}");
}
