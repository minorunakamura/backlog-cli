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
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", h.root.join("bin").display()),
        )
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
        "Select an issue",
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
    let mut routes = json!({"routes": {
        "/api/v2/space":{"body":"{\"name\":\"Actual space\"}"},
        "/api/v2/projects":{"body":json!([
            {"id":123456789012345678901234567890_u128.to_string().parse::<serde_json::Number>().unwrap(),"projectKey":"A","name":"Alpha"},
            {"id":2,"projectKey":"B","name":"Beta"}
        ]).to_string()},
        "/api/v2/issues/count":{"body":"{\"count\":1}"},
        "/api/v2/issues":{"body":"[{\"id\":1,\"issueKey\":\"B-1\",\"summary\":\"Selected project issue\",\"status\":{\"name\":\"open\"}}]"},
        "/api/v2/projects/2":{"body":"{\"textFormattingRule\":\"markdown\"}"}
    }});
    // List-only scenarios still need successful automatic previews, not unrelated 404 popups.
    for n in 1..=99 {
        let key = format!("B-{n}");
        routes["routes"][format!("/api/v2/issues/{key}")] =
            json!({"body":detail_issue(&key, "Preview body").to_string()});
        routes["routes"][format!("/api/v2/issues/{key}/comments")] = json!({"body":"[]"});
    }
    routes
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
    assert_eq!(
        result["browse_request_counts"],
        json!([2, 2, 4, 4]),
        "{result}"
    );
    let snapshots = result["snapshots"].as_array().unwrap();
    assert!(snapshots[0].as_str().unwrap().contains("> Project List"));
    assert!(snapshots[2].as_str().unwrap().contains("> Project List"));
    let requests = browse_requests(&h);
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
                assert_eq!(
                    result["browse_request_counts"][0], initial_requests,
                    "{context}"
                );
                if cancel {
                    assert_eq!(
                        result["browse_request_counts"][1], initial_requests,
                        "{context}"
                    );
                }
                let requests = browse_requests(&h);
                if status == 200 {
                    assert_eq!(result["code"], 0, "{context}");
                    assert_eq!(result["stderr"], "", "{context}");
                    let screen = result["screen"].as_str().unwrap();
                    for text in ["Actual space", "Project: B", "Selected project issue"] {
                        assert!(screen.contains(text), "missing {text}: {context}");
                    }
                    assert!(!screen.contains("Loading projects"), "{context}");
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
        result["browse_request_counts"],
        json!([4, 5, 5, 5, 6, 6, 6]),
        "{result}"
    );
    let requests = browse_requests(&h);
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
            assert_eq!(result["browse_request_counts"], json!([2, 2]));
        } else {
            let result = scripted(&h, json!([{ "wait":"Selected project issue", "keys":"q" }]));
            assert_eq!(result["browse_request_counts"], json!([4]));
            let path = browse_requests(&h)[2]["path"].as_str().unwrap().to_owned();
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
            {"wait":"Failed:","keys":" "},
            {"wait":"> P11  Project 11","keys":"R"},
            {"wait":"Refreshed projects","keys":"q"}
        ]),
    );
    let snapshots = result["snapshots"].as_array().unwrap();
    assert!(
        snapshots[2].as_str().unwrap().contains("> P11  Project 11"),
        "{result}"
    );
    for (index, snapshot) in snapshots.iter().enumerate() {
        if index == 2 {
            continue; // The popup covers the list row; check retention again after dismissal.
        }
        assert!(
            snapshot
                .as_str()
                .unwrap()
                .contains("Selected project issue"),
            "{result}"
        );
    }
    assert_eq!(result["browse_request_counts"], json!([4, 4, 5, 5, 6]));
    for request in browse_requests(&h).iter().skip(4) {
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
            {"wait":"Failed:","keys":" pn"},
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
        snapshots[3].as_str().unwrap().contains("total 25"),
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
        result["browse_request_counts"],
        json!([4, 5, 7, 7, 9, 9, 10]),
        "{result}"
    );
    assert!(
        browse_requests(&h)[9]["path"]
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
        result["browse_request_counts"],
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
        assert_eq!(result["browse_request_counts"], json!([4, 4]), "{result}");
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
        result["browse_request_counts"],
        json!([5, 6, 6, 9, 9, 12, 13]),
        "{result}"
    );
    let requests = browse_requests(&h);
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
    assert_eq!(result["browse_request_counts"], json!([4, 5]), "{result}");
}

fn browse_requests(h: &Harness) -> Vec<Value> {
    h.requests()
        .into_iter()
        .filter(|request| {
            let path = request["path"].as_str().unwrap();
            (!path.starts_with("/api/v2/issues/") || path.starts_with("/api/v2/issues/count?"))
                && !path
                    .strip_prefix("/api/v2/projects/")
                    .is_some_and(|tail| !tail.contains('/'))
        })
        .collect()
}

fn detail_issue(key: &str, body: &str) -> Value {
    json!({"id":1,"projectId":2,"issueKey":key,"summary":format!("Detail {key}"),
        "description":body,"issueType":{"name":"Task"},"priority":{"name":"Medium"},
        "assignee":null,"status":{"name":"Open"},"resolution":null,"startDate":null,
        "dueDate":null,"milestone":[{"name":"v1"},{"name":"v2"}],
        "created":"2026-01-01T00:00:00Z","updated":"2026-02-02T00:00:00Z",
        "url":"https://evil.invalid/test-secret"})
}
fn detail_routes() -> Value {
    let mut routes = browsing_routes();
    routes["routes"]["/api/v2/issues/count"] = json!({"body":"{\"count\":3}"});
    routes["routes"]["/api/v2/issues"] = json!({"body":issue_items(1,3)});
    routes["routes"]["/api/v2/projects/2"] =
        json!({"body":"{\"textFormattingRule\":\"markdown\"}"});
    for n in 1..=3 {
        let key = format!("B-{n}");
        routes["routes"][format!("/api/v2/issues/{key}")] = json!({"body":detail_issue(&key, &format!("**Body {key}** test-secret\u{1b}]52;c;bad\u{7}")).to_string()});
    }
    routes
}

#[test]
fn preview_debounces_selection_and_enter_reuses_safely_rendered_detail() {
    let h = Harness::new(detail_routes());
    settings(&h, "B");
    let result = scripted(
        &h,
        json!([
            {"wait":"Issue 01","pause":0.02,"keys":"lj"},
        {"wait":"> B-2","pause":0.07,"keys":"j\r"},
            {"wait":"TITLE: Detail B-3","keys":"G"},
        {"wait":"Body B-3","keys":"g"},
        {"wait":"TITLE: Detail B-3","keys":"q"}
        ]),
    );
    let snapshot = result["snapshots"].to_string();
    for text in [
        "> Issue Detail",
        "TITLE",
        "TYPE",
        "PRIORITY",
        "ASSIGNEE",
        "STATUS",
        "RESOLUTION",
        "START",
        "DUE",
        "MILESTONES",
        "CREATED",
        "UPDATED",
        "BODY",
        "v1, v2",
    ] {
        assert!(snapshot.contains(text), "missing {text}: {result}");
    }
    let output = result["stdout"].as_str().unwrap();
    assert!(
        !output.contains("test-secret") && !output.contains("\u{1b}]52"),
        "{result}"
    );
    assert!(output.contains("[REDACTED]"), "{result}");
    let requests = h.requests();
    assert_eq!(requests.len(), 7, "{requests:?}");
    let detail = requests
        .iter()
        .find(|r| r["path"] == "/api/v2/issues/B-3")
        .unwrap();
    assert!(requests.iter().any(|r| r["path"] == "/api/v2/projects/2"));
    let debounce = detail["at"].as_f64().unwrap() - result["sent_at"][1].as_f64().unwrap();
    assert!(
        (0.12..1.0).contains(&debounce),
        "expected approximately 150ms debounce, got {debounce}"
    );
}

#[test]
fn preview_without_a_document_sanitizes_the_selected_key() {
    let mut routes = detail_routes();
    routes["routes"]["/api/v2/issues"] = json!({"body":json!([
        {"id":1,"issueKey":"test-secret\u{1b}]52;c;bad\u{7}-1","summary":"Selected issue","status":{"name":"open"}}
    ]).to_string()});
    let h = Harness::new(routes);
    settings(&h, "B");
    let result = scripted(
        &h,
        json!([
            {"wait":"Preview failed","keys":"l\r\r"},
            {"wait":"> Issue Detail","keys":"q"}
        ]),
    );
    let output = result["stdout"].as_str().unwrap();
    assert!(
        !output.contains("test-secret") && !output.contains("\u{1b}]52"),
        "{result}"
    );
    assert!(
        result["snapshots"][1]
            .as_str()
            .unwrap()
            .contains("[REDACTED]")
    );
    assert_eq!(h.requests().len(), 4, "{result}");
}

#[test]
fn preview_cancelled_before_debounce_has_no_request_or_permanent_loading() {
    let h = Harness::new(detail_routes());
    settings(&h, "B");
    let result = scripted(
        &h,
        json!([
            {"wait":"Issue 01","pause":0.01,"keys":"l\r\u{1b}"},
            {"wait":"Preview cancelled","pause":0.3,"keys":"q"}
        ]),
    );
    assert_eq!(h.requests().len(), 4, "{result}");
    let snapshot = result["snapshots"][1].as_str().unwrap();
    assert!(
        snapshot.contains("B-1") && !snapshot.contains("Loading preview"),
        "{result}"
    );
}

#[test]
fn detail_reload_replaces_only_after_issue_and_rule_success_and_keeps_list_focus_and_scroll_on_failure()
 {
    let mut routes = detail_routes();
    let old = detail_issue(
        "B-1",
        &(1..=30)
            .map(|n| format!("Old body line {n:02}\n"))
            .collect::<String>(),
    );
    routes["routes"]["/api/v2/issues/B-1"] = json!({"sequence":[
        {"body":old.to_string()}, {"status":500},
        {"body":detail_issue("B-1", "Discarded new body").to_string()},
        {"body":detail_issue("B-1", "**Fresh body**").to_string()}
    ]});
    routes["routes"]["/api/v2/projects/2"] = json!({"sequence":[
        {"body":"{\"textFormattingRule\":\"markdown\"}"}, {"status":403,"delay":0.2},
        {"body":"{\"textFormattingRule\":\"unknown-format\"}"}
    ]});
    let h = Harness::new(routes);
    settings(&h, "B");
    let result = scripted(
        &h,
        json!([
            {"wait":"TITLE: Detail B-1","keys":"l\rG"},
            {"wait":"Old body line 30","keys":"R"},
            {"wait":"Failed:","keys":"R"},
            {"wait":"Loading... Esc: cancel","pause":0.01,"keys":""},
            {"wait":"Failed:","keys":" "},
            {"wait":"Old body line 30","keys":"R"},
            {"wait":"TITLE: Detail B-1","keys":"G"},
            {"wait":"**Fresh body**","keys":"q"}
        ]),
    );
    for index in [2, 3, 4] {
        let snapshot = result["snapshots"][index].as_str().unwrap();
        assert!(
            snapshot.contains("Old body line 30") && snapshot.contains("> Issue Detail"),
            "{result}"
        );
        assert!(!snapshot.contains("Discarded new body"), "{result}");
    }
    assert!(
        !result["snapshots"][3].as_str().unwrap().contains("Failed:"),
        "R must clear the old notification before retrying: {result}"
    );
    for index in [0, 1, 5, 6, 7] {
        assert!(
            result["snapshots"][index]
                .as_str()
                .unwrap()
                .contains("Issue 01"),
            "{result}"
        );
    }
    assert!(
        result["snapshots"][5]
            .as_str()
            .unwrap()
            .contains("Old body line 30"),
        "dismissal must retain scroll: {result}"
    );
    assert_eq!(browse_requests(&h).len(), 4, "{result}");
    assert_eq!(h.requests().len(), 12, "{result}");
}

#[test]
fn selection_change_hides_other_body_immediately_and_late_preview_cannot_clear_new_busy_or_failure()
{
    for old_status in [200, 500] {
        let mut routes = detail_routes();
        routes["routes"]["/api/v2/issues/B-2"]["header_delay"] = json!(0.45);
        routes["routes"]["/api/v2/issues/B-2"]["status"] = json!(old_status);
        routes["routes"]["/api/v2/issues/B-3"] = json!({"status":500,"header_delay":0.85});
        let h = Harness::new(routes);
        settings(&h, "B");
        let result = scripted(
            &h,
            json!([
                {"wait":"TITLE: Detail B-1","keys":"lj"},
                {"wait":"B-2","pause":0.19,"keys":"j"},
                {"wait":"B-3","pause":0.6,"keys":""},
                {"wait":"Preview failed","keys":"q"}
            ]),
        );
        for index in [1, 2, 3] {
            let snapshot = result["snapshots"][index].as_str().unwrap();
            assert!(
                !snapshot.contains("TITLE: Detail B-1") && !snapshot.contains("TITLE: Detail B-2"),
                "{result}"
            );
        }
        assert!(
            result["snapshots"][2]
                .as_str()
                .unwrap()
                .contains("Loading... Esc: cancel"),
            "{result}"
        );
        assert!(
            result["snapshots"][3].as_str().unwrap().contains("B-3"),
            "{result}"
        );
        assert_eq!(h.requests().len(), 11, "{result}");
    }
}

fn browser_stub(h: &Harness) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir(h.root.join("bin")).unwrap();
    let path = h.root.join("bin/open");
    std::fs::write(&path, "#!/bin/sh\nprintf '%s\\n' \"$#\" \"$@\" >> open-args\ncase \"$1\" in *B-2) exit 7;; esac\n").unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
}

#[test]
fn ctrl_o_uses_only_origin_and_selected_key_without_shell_and_failure_keeps_selection() {
    let h = Harness::new(detail_routes());
    settings(&h, "B");
    browser_stub(&h);
    let result = scripted(
        &h,
        json!([
            {"wait":"TITLE: Detail B-1","keys":"l\u{f}"},
            {"wait":"> B-1","wait_file":"open-args","keys":"j\u{f}"},
            {"wait":"Browser launch failed","keys":"\r"},
            {"wait":"> Issue List","keys":"\r"},
            {"wait":"> Issue Detail","keys":"\u{1b}"},
            {"wait":"> Issue List","keys":"q"}
        ]),
    );
    let failed = result["snapshots"][2].as_str().unwrap();
    assert!(failed.contains("> B-2"), "{result}");
    let rows: Vec<_> = failed.lines().collect();
    assert_eq!(rows[10].chars().nth(12), Some('┌'), "{result}");
    assert!(rows[11].contains("Browser launch failed"), "{result}");
    assert_eq!(rows[12].chars().nth(12), Some('└'), "{result}");
    assert_eq!(failed.matches("Browser launch failed").count(), 1);
    assert!(!failed.contains("ERROR") && !failed.contains("OK"));
    assert!(
        result["snapshots"][3]
            .as_str()
            .unwrap()
            .contains("> Issue List")
    );
    assert!(
        !result["snapshots"][3]
            .as_str()
            .unwrap()
            .contains("Browser launch failed")
    );
    assert!(
        result["snapshots"][4]
            .as_str()
            .unwrap()
            .contains("> Issue Detail")
    );
    assert!(result["snapshots"][5].as_str().unwrap().contains("> B-2"));
    assert_eq!(
        std::fs::read_to_string(h.root.join("open-args")).unwrap(),
        format!("1\n{}/view/B-1\n1\n{}/view/B-2\n", h.origin, h.origin)
    );
    assert!(!result["stdout"].as_str().unwrap().contains("evil.invalid"));
    assert_eq!(browse_requests(&h).len(), 4);
}

#[test]
fn browser_error_popup_survives_cancelled_old_response_resize_and_clears_on_selection_before_interrupt()
 {
    let mut routes = detail_routes();
    routes["routes"]["/api/v2/issues/B-1"] = json!({"sequence":[
        {"body":detail_issue("B-1", "Retained body").to_string()},
        {"status":500,"header_delay":0.45}
    ]});
    let h = Harness::new(routes);
    settings(&h, "B");
    browser_stub(&h);
    std::fs::write(h.root.join("bin/open"), "#!/bin/sh\nexit 7\n").unwrap();
    std::fs::write(
        h.root.join("shell-steps.json"),
        json!([
            {"wait":"TITLE: Detail B-1","keys":"l\rR"},
            {"wait":"Loading... Esc: cancel","keys":"\u{1b}"},
            {"wait":"> Issue List","keys":"\u{f}"},
            {"wait":"Browser launch failed","pause":0.65,"keys":"j"},
            {"wait":"> B-2","pause":0.3,"keys":"\u{f}"},
            {"wait":"Browser launch failed","resize":[12,40]},
            {"wait":"Browser launch failed","resize":[11,39]},
            {"wait":"Terminal too small","keys":"\r\u{1b} "},
            {"wait":"Terminal too small","pause":0.2,"resize":[12,40]},
            {"wait":"Browser launch failed","keys":"\u{3}"}
        ])
        .to_string(),
    )
    .unwrap();
    let result = shell(&h, "scripted", &[]);
    assert_eq!(result["code"], 130, "{result}");
    assert_eq!(result["restored"], true, "{result}");
    assert!(result["stderr"].as_str().unwrap().contains("interrupted"));
    for index in [3, 5, 6, 9] {
        let snapshot = result["snapshots"][index].as_str().unwrap();
        assert_eq!(
            snapshot.matches("Browser launch failed").count(),
            1,
            "{result}"
        );
        assert!(
            !snapshot.contains("API request was rejected"),
            "old response must not replace the popup: {result}"
        );
    }
    let moved = result["snapshots"][4].as_str().unwrap();
    assert!(
        moved.contains("> B-2"),
        "j still changes selection: {result}"
    );
    assert!(
        !moved.contains("Browser launch failed"),
        "j also clears the old notification: {result}"
    );
    assert_eq!(browse_requests(&h).len(), 4, "{result}");
    assert_eq!(h.requests().len(), 11, "{result}");
}

#[test]
fn ctrl_o_rejects_invalid_list_keys_and_secret_urls_without_spawning_browser_or_issue_get() {
    for key in ["https://evil.invalid/x", "../../x", "42", "B-1\n", "B-1"] {
        let mut routes = detail_routes();
        routes["routes"]["/api/v2/issues"] = json!({"body":json!([
            {"id":1,"issueKey":key,"summary":"Selected issue","status":{"name":"open"}}
        ]).to_string()});
        let h = Harness::new(routes);
        settings(&h, "B");
        browser_stub(&h);
        if key == "B-1" {
            // The configured API key is "1", so the otherwise valid URL includes the secret.
            h.config(json!({"profiles":{"default":{"spaceUrl":h.origin,"apiKeyEnv":"NO_COLOR"}}}));
        }
        // Cancel unrelated previews before Ctrl+O so their errors cannot replace its notification.
        let result = scripted(
            &h,
            json!([
                {"wait":"Project: B","keys":"l\u{1b}"},
                {"wait":"h/l Tab/S-Tab:Project","keys":"\u{f}"},
                {"wait":if key == "B-1" { "Cannot open a URL" } else { "Invalid selected issue key" },"keys":" "},
                {"wait":"> Issue List","keys":"q"}
            ]),
        );
        assert!(!h.root.join("open-args").exists(), "{key}: {result}");
        if key != "B-1" {
            assert_eq!(h.requests().len(), 4, "{key}: {result}");
        }
        assert!(
            result["snapshots"][3]
                .as_str()
                .unwrap()
                .contains("> Issue List")
        );
    }
}

#[test]
fn closed_detail_reload_cannot_replace_document_or_busy_after_escape_and_resize_keeps_tail() {
    let mut routes = detail_routes();
    let old = detail_issue(
        "B-1",
        &(1..=30)
            .map(|n| format!("Retained line {n:02} 日本語\n"))
            .collect::<String>(),
    );
    routes["routes"]["/api/v2/issues/B-1"] = json!({"sequence":[
        {"body":old.to_string()},
        {"body":detail_issue("B-1", "Late closed body").to_string(),"header_delay":0.4}
    ]});
    let h = Harness::new(routes);
    settings(&h, "B");
    let result = scripted(
        &h,
        json!([
            {"wait":"TITLE: Detail B-1","keys":"l\rG"},
            {"wait":"Retained line 30","keys":"R"},
            {"wait":"Loading... Esc: cancel","keys":"\u{1b}"},
            {"wait":"> Issue List","pause":0.6,"keys":"\r"},
            {"wait":"Retained line 30","resize":[12,40]},
            {"wait":"Retained line 20","keys":"G"},
            {"wait":"Retained line 30","keys":"g"},
            {"wait":"TITLE: Detail B-1","keys":"G"},
            {"wait":"Retained line 30","resize":[60,160]},
            {"wait":"Retained line 30","keys":"q"}
        ]),
    );
    for index in [3, 4, 5, 6, 7, 8, 9] {
        let snapshot = result["snapshots"][index].as_str().unwrap();
        assert!(
            !snapshot.contains("Late closed body") && !snapshot.contains("Loading... Esc: cancel"),
            "{result}"
        );
    }
    assert!(
        result["snapshots"][9]
            .as_str()
            .unwrap()
            .contains("TITLE: Detail B-1"),
        "{result}"
    );
    assert_eq!(h.requests().len(), 8, "{result}");
}

#[test]
fn superseded_detail_reload_failure_cannot_clear_new_reload_busy() {
    let mut routes = detail_routes();
    let mut fresh = detail_issue("B-1", "Latest body");
    fresh["summary"] = json!("Latest detail");
    routes["routes"]["/api/v2/issues/B-1"] = json!({"sequence":[
        {"body":detail_issue("B-1", "Retained body").to_string()},
        {"status":500,"header_delay":0.35},
        {"body":fresh.to_string(),"header_delay":0.8}
    ]});
    let h = Harness::new(routes);
    settings(&h, "B");
    let result = scripted(
        &h,
        json!([
            {"wait":"TITLE: Detail B-1","keys":"l\rR"},
            {"wait":"Loading... Esc: cancel","keys":"R"},
            {"wait":"Loading... Esc: cancel","pause":0.45,"keys":""},
            {"wait":"TITLE: Latest detail","keys":"q"}
        ]),
    );
    let snapshot = result["snapshots"][2].as_str().unwrap();
    assert!(
        snapshot.contains("Loading... Esc: cancel") && snapshot.contains("TITLE: Detail B-1"),
        "{result}"
    );
    assert!(!snapshot.contains("Failed:"), "{result}");
    assert_eq!(h.requests().len(), 10, "{result}");
    assert_eq!(browse_requests(&h).len(), 4);
}

#[test]
fn preview_rejects_a_different_issue_response_without_showing_its_body_or_fetching_its_project() {
    let mut routes = detail_routes();
    routes["routes"]["/api/v2/issues/B-1"] =
        json!({"body":detail_issue("B-2", "Wrong issue body").to_string()});
    let h = Harness::new(routes);
    settings(&h, "B");
    let result = scripted(
        &h,
        json!([
            {"wait":"Preview failed","keys":"q"}
        ]),
    );
    assert!(
        !result["stdout"]
            .as_str()
            .unwrap()
            .contains("Wrong issue body")
    );
    assert!(result["screen"].as_str().unwrap().contains("B-1"));
    assert_eq!(h.requests().len(), 6, "{result}");
}

#[test]
fn documented_detail_fixture_runs_through_real_bl_without_opening_browser() {
    let h = Harness::new(serde_json::from_str(include_str!("fixtures/tui_detail.json")).unwrap());
    settings(&h, "B");
    browser_stub(&h);
    std::fs::write(
        h.root.join("bin/open"),
        "#!/bin/sh\nprintf '%s\\n' \"$#\" \"$@\" >> open-args\nexit 7\n",
    )
    .unwrap();
    let result = scripted(
        &h,
        serde_json::from_str(include_str!("fixtures/tui_detail_steps.json")).unwrap(),
    );
    assert_eq!(
        std::fs::read_to_string(h.root.join("open-args")).unwrap(),
        format!("1\n{}/view/B-3\n", h.origin)
    );
    assert_eq!(h.requests().len(), 10, "{result}");
    assert!(!result["stdout"].as_str().unwrap().contains("test-secret"));
    assert!(
        result["snapshots"][5]
            .as_str()
            .unwrap()
            .contains("Local body line 30"),
        "{result}"
    );
    assert!(
        result["snapshots"][6]
            .as_str()
            .unwrap()
            .contains("KEY: B-3"),
        "{result}"
    );
    assert!(
        !result["snapshots"][6].as_str().unwrap().contains("Failed:"),
        "R must clear the notification and show the refreshed document: {result}"
    );
}

fn assert_search_fields_blank(snapshot: &str) {
    let rows: Vec<_> = snapshot.lines().collect();
    let top = rows.iter().position(|row| row.contains("┌Search")).unwrap();
    // The simple VT decoder can retain wide-character continuation cells after popup closure.
    for row in &rows[top + 1..=top + 3] {
        for value in [
            "Any",
            "open",
            "closed",
            "Alice",
            "Bob",
            "v1",
            "Cached status",
        ] {
            assert!(
                !row.contains(value),
                "cleared fields must not display {value}: {snapshot}"
            );
        }
    }
}

#[test]
fn search_dialog_edits_keyword_without_searching_and_clear_stays_clear_on_reopen_and_reload() {
    let mut routes = browsing_routes();
    routes["routes"]["/api/v2/projects/2/statuses"] =
        json!({"body":"[{\"id\":1,\"name\":\"open\"}]"});
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
            {"wait":"Selected project issue","keys":"l/"},
            {"wait":"Search","keys":"deleted\u{15}qhljk"},
            {"wait":"qhljk","keys":"\r"},
            {"wait":"> Issue List","keys":"/"},
            {"wait":"qhljk","keys":"\t\u{0015}\r"},
            {"wait":"> Issue List","keys":"/"},
            {"wait":"Search","keys":"\u{001b}"},
        {"wait":"> Issue List","keys":"R"},
            {"wait":"Selected project issue","keys":"q"}
        ]),
    );
    let snapshots = result["snapshots"].as_array().unwrap();
    assert!(
        !snapshots[1].as_str().unwrap().contains("q:quit")
            && !snapshots[1].as_str().unwrap().contains("H:Left"),
        "keyword q is text, not quit: {result}"
    );
    assert!(snapshots[1].as_str().unwrap().contains("open"), "{result}");
    assert_search_fields_blank(snapshots[6].as_str().unwrap());
    let requests = h.requests();
    let queries: Vec<_> = requests
        .iter()
        .filter(|r| {
            let path = r["path"].as_str().unwrap();
            path.starts_with("/api/v2/issues?") || path.starts_with("/api/v2/issues/count?")
        })
        .map(|r| r["path"].as_str().unwrap())
        .collect();
    assert_eq!(queries.len(), 8, "{queries:?}");
    assert!(queries[..2].iter().all(|q| q.contains("statusId%5B%5D=1")));
    assert!(
        queries[2..4]
            .iter()
            .all(|q| q.contains("keyword=qhljk") && q.contains("statusId%5B%5D=1"))
    );
    assert!(
        queries[4..]
            .iter()
            .all(|q| !q.contains("keyword=") && !q.contains("statusId"))
    );
}

#[test]
fn search_dialog_multiselects_all_fields_and_applies_identical_count_list_filters() {
    let mut routes = browsing_routes();
    let huge: serde_json::Number = "123456789012345678901234567890".parse().unwrap();
    for (path, items) in [
        (
            "statuses",
            json!([{"id":1,"name":"open"},{"id":2,"name":"closed"}]),
        ),
        (
            "users",
            json!([{"id":7,"name":"Alice"},{"id":8,"name":"Bob"}]),
        ),
        (
            "versions",
            json!([{"id":huge,"name":"v1"},{"id":10,"name":"v2"}]),
        ),
    ] {
        routes["routes"][format!("/api/v2/projects/2/{path}")] = json!({"body":items.to_string()});
    }
    let h = Harness::new(routes);
    settings(&h, "B");
    let result = scripted(
        &h,
        json!([
            {"wait":"Selected project issue","keys":"l/"},
            {"wait":"Search","keys":"fix qhljk\t"},
            {"wait":"[ ] open","keys":" j "},
            {"wait":"[x] closed","keys":"\t"},
            {"wait":"[ ] Alice","keys":" \u{1b}[B "},
            {"wait":"[x] Bob","keys":"\u{7}"},
            {"wait":"[ ] v1","keys":" j "},
            {"wait":"[x] v2","keys":"\u{1b}[Z"},
            {"wait":"[x] Alice","keys":"\u{1b}"},
            {"wait":"Alice (7)","keys":"\u{b}"},
            {"wait":"fix qhljk","keys":"\r"},
            {"wait":"> Issue List","keys":"h\r"},
            {"wait":"> Project List","keys":"l/"},
            {"wait":"v1 (","keys":"\u{1b}"},
            {"wait":"> Issue List","keys":"q"}
        ]),
    );
    assert!(
        result["snapshots"][11]
            .as_str()
            .unwrap()
            .contains("S:2 A:2 G:2"),
        "summary must show all applied fields: {result}"
    );
    assert!(
        result["snapshots"][9].as_str().unwrap().contains("Search"),
        "nested Esc must retain dialog: {result}"
    );
    let rows: Vec<_> = result["snapshots"][9].as_str().unwrap().lines().collect();
    let top = rows.iter().position(|row| row.contains("┌Search")).unwrap();
    for (row, value) in [
        (top + 1, "open (1), closed (2)"),
        (top + 1, "Alice (7), Bob (8)"),
        (top + 3, "v1 ("),
    ] {
        assert!(
            rows[row].contains(value),
            "selected value must share its label's row: {result}"
        );
    }
    let searches: Vec<_> = h
        .requests()
        .into_iter()
        .filter(|r| {
            r["path"].as_str().unwrap().starts_with("/api/v2/issues?")
                || r["path"]
                    .as_str()
                    .unwrap()
                    .starts_with("/api/v2/issues/count?")
        })
        .collect();
    assert_eq!(
        searches.len(),
        4,
        "editing and same-project confirm must not search: {searches:?}"
    );
    let query = |r: &Value| {
        reqwest::Url::parse(&format!("https://localhost{}", r["path"].as_str().unwrap()))
            .unwrap()
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect::<Vec<_>>()
    };
    let expected = vec![
        ("projectId[]", "2"),
        ("keyword", "fix qhljk"),
        ("statusId[]", "1"),
        ("statusId[]", "2"),
        ("assigneeId[]", "7"),
        ("assigneeId[]", "8"),
        ("milestoneId[]", "123456789012345678901234567890"),
        ("milestoneId[]", "10"),
        ("sort", "updated"),
        ("order", "desc"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v.to_owned()))
    .collect::<Vec<_>>();
    assert_eq!(query(&searches[2]), expected);
    let mut list = expected;
    list.extend([("count".into(), "20".into()), ("offset".into(), "0".into())]);
    assert_eq!(query(&searches[3]), list);
}

#[test]
fn cross_project_dialog_requires_keyword_or_self_ignores_defaults_and_returns_to_concrete_project()
{
    let mut routes = browsing_routes();
    routes["routes"]["/api/v2/projects/2/statuses"] =
        json!({"body":"[{\"id\":1,\"name\":\"open\"}]"});
    routes["routes"]["/api/v2/users/myself"] = json!({"body":"{\"id\":7}"});
    routes["routes"]["/api/v2/issues"] = json!({"sequence":[
        {"body":issue_items(1,1)},
        {"body":json!([{"id":1,"issueKey":"A-1","summary":"Alpha result","status":{"name":"open"}},{"id":2,"issueKey":"B-2","summary":"Beta result","status":{"name":"open"}}]).to_string()},
        {"body":issue_items(3,1)}, {"body":issue_items(4,1)}
    ]});
    routes["routes"]["/api/v2/issues/A-1"] =
        json!({"body":detail_issue("A-1", "Alpha body").to_string()});
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
            {"wait":"Issue 01","keys":"\u{1}"},
            {"wait":"Cross-project Search","keys":"   \r"},
            {"wait":"Cross-project search requires","keys":"\u{15}keyword\u{13}\u{7}\r"},
            {"wait":"Alpha result","keys":"/"},
            {"wait":"keyword","keys":"\u{15}\t"},
            {"wait":"[ ] 自 分","keys":" \r\r"},
            {"wait":"Issue 03","keys":"c"},
            {"wait":"Select a concrete project","keys":" "},
            {"wait":"> Issue List","keys":"/"},
            {"wait":"Cross-project Search","keys":"\u{1b}"},
            {"wait":"> Issue List","keys":"h\r"},
            {"wait":"Issue 04","keys":"l/"},
            {"wait":"open","keys":"\u{1b}"},
            {"wait":"> Issue List","keys":"q"}
        ]),
    );
    assert!(
        result["snapshots"][3]
            .as_str()
            .unwrap()
            .contains("Beta result"),
        "{result}"
    );
    assert!(
        result["snapshots"][9].as_str().unwrap().contains("自")
            && result["snapshots"][9].as_str().unwrap().contains('分')
            && !result["snapshots"][9].as_str().unwrap().contains("self"),
        "{result}"
    );
    let searches: Vec<_> = h
        .requests()
        .into_iter()
        .filter(|r| {
            r["path"].as_str().unwrap().starts_with("/api/v2/issues?")
                || r["path"]
                    .as_str()
                    .unwrap()
                    .starts_with("/api/v2/issues/count?")
        })
        .collect();
    assert_eq!(
        searches.len(),
        8,
        "blank and editing must not count/list: {searches:?}"
    );
    for request in &searches[2..6] {
        let q = request["path"].as_str().unwrap();
        assert!(
            !q.contains("projectId") && !q.contains("statusId") && !q.contains("milestoneId"),
            "{q}"
        );
    }
    assert!(
        searches[2]["path"]
            .as_str()
            .unwrap()
            .contains("keyword=keyword")
    );
    assert!(
        searches[4]["path"]
            .as_str()
            .unwrap()
            .contains("assigneeId%5B%5D=7")
    );
    assert!(
        searches[6]["path"]
            .as_str()
            .unwrap()
            .contains("statusId%5B%5D=1")
    );
    assert_eq!(
        h.requests()
            .iter()
            .filter(|r| r["path"] == "/api/v2/users/myself")
            .count(),
        1
    );
}

#[test]
fn search_dialog_cancel_and_count_or_list_failure_preserve_applied_history_selection_and_draft() {
    for fail_count in [true, false] {
        let mut routes = browsing_routes();
        routes["routes"]["/api/v2/issues/count"] = json!({"sequence":[
            {"body":"{\"count\":25}"},
            if fail_count { json!({"status":403}) } else { json!({"body":"{\"count\":999}"}) }
        ]});
        routes["routes"]["/api/v2/issues"] = json!({"sequence":[
            {"body":issue_items(1,20)}, {"body":issue_items(21,3)}, {"status":500}
        ]});
        let h = Harness::new(routes);
        settings(&h, "B");
        let result = scripted(
            &h,
            json!([
                {"wait":"Issue 01","keys":"ln"},
                {"wait":"Issue 21","keys":"jj/"},
                {"wait":"Search","keys":"cancelled"},
                {"wait":"cancelled","keys":"\u{1b}"},
                {"wait":"> B-23","keys":"/"},
                {"wait":"Search","keys":"failed qhljk\r"},
                {"wait":"Failed:","keys":"\u{b}"},
                {"wait":"failed qhljk","keys":"\u{1b}"},
                {"wait":"> B-23","keys":"LlHLHpn"},
                {"wait":"> B-23","keys":"q"}
            ]),
        );
        let snapshots = result["snapshots"].as_array().unwrap();
        assert!(
            !snapshots[5].as_str().unwrap().contains("cancelled"),
            "cancel must discard draft: {result}"
        );
        assert!(
            snapshots[7].as_str().unwrap().contains("failed qhljk"),
            "failure must keep draft: {result}"
        );
        for index in [4, 8, 9] {
            let screen = snapshots[index].as_str().unwrap();
            assert!(
                screen.contains("total 25 | page 2") && screen.contains("> B-23"),
                "{result}"
            );
        }
        let requests = browse_requests(&h);
        assert_eq!(
            requests.len(),
            if fail_count { 6 } else { 7 },
            "{requests:?}"
        );
    }
}

#[test]
fn search_dialog_late_candidate_success_or_failure_cannot_replace_current_candidates_or_clear_busy()
{
    for failed in [false, true] {
        let mut routes = browsing_routes();
        routes["routes"]["/api/v2/projects/2/statuses"] = if failed {
            json!({"status":500,"header_delay":0.35})
        } else {
            json!({"body":"[{\"id\":1,\"name\":\"STALE STATUS\"}]","header_delay":0.35})
        };
        routes["routes"]["/api/v2/projects/2/users"] =
            json!({"body":"[{\"id\":7,\"name\":\"Current user\"}]","header_delay":0.8});
        let h = Harness::new(routes);
        settings(&h, "B");
        let result = scripted(
            &h,
            json!([
                {"wait":"Selected project issue","keys":"l/\t"},
                {"wait":"Loading candidates","keys":"\u{1}"},
                {"wait":"Loading candidates","pause":0.5,"keys":""},
                {"wait":"[ ] Current user","keys":"\u{1b}"},
                {"wait":"Search","keys":"\u{1b}"},
                {"wait":"> Issue List","keys":"q"}
            ]),
        );
        for index in [2, 3] {
            let screen = result["snapshots"][index].as_str().unwrap();
            assert!(
                !screen.contains("STALE STATUS") && !screen.contains("Failed:"),
                "{result}"
            );
        }
        assert!(
            result["snapshots"][2]
                .as_str()
                .unwrap()
                .contains("Loading candidates"),
            "{result}"
        );
    }
}

#[test]
fn search_dialog_cancelled_apply_cannot_change_reopened_draft_or_new_apply_busy() {
    for failed in [false, true] {
        let mut routes = browsing_routes();
        routes["routes"]["/api/v2/issues/count"] = json!({"sequence":[
            {"body":"{\"count\":1}"}, {"body":"{\"count\":2}"}, {"body":"{\"count\":3}"}
        ]});
        routes["routes"]["/api/v2/issues"] = json!({"sequence":[
            {"body":issue_items(1,1)},
            if failed { json!({"status":500,"header_delay":0.35}) } else { json!({"body":issue_items(2,1),"header_delay":0.35}) },
            {"body":issue_items(3,1),"header_delay":0.8}
        ]});
        let h = Harness::new(routes);
        settings(&h, "B");
        let result = scripted(
            &h,
            json!([
                {"wait":"Issue 01","keys":"l/old\r"},
                {"wait":"Applying...","keys":"ignored\u{15}\r\u{1b}"},
                {"wait":"> Issue List","keys":"/new"},
                {"wait":"new","keys":"\r"},
                {"wait":"Applying...","pause":0.5,"keys":""},
                {"wait":"Issue 03","keys":"/"},
                {"wait":"new","keys":"\u{1b}"},
                {"wait":"> Issue List","keys":"q"}
            ]),
        );
        let busy = result["snapshots"][4].as_str().unwrap();
        assert!(
            busy.contains("Applying...") && !busy.contains("Failed:"),
            "{result}"
        );
        let requests = browse_requests(&h);
        assert_eq!(
            requests.len(),
            8,
            "duplicates/edits during apply are ignored: {requests:?}"
        );
        assert!(
            requests[6]["path"]
                .as_str()
                .unwrap()
                .contains("keyword=new")
        );
        assert!(
            result["snapshots"][5]
                .as_str()
                .unwrap()
                .contains("total 3 | page 1"),
            "{result}"
        );
    }
}

#[test]
fn search_dialog_compact_scrolls_full_selected_values_and_popup_rows_without_wrapping() {
    let mut routes = browsing_routes();
    routes["routes"]["/api/v2/projects/2/statuses"] = json!({"body":json!([
        {"id":1,"name":"First status with a long readable label"},
        {"id":2,"name":"Second status with another long readable label"},
        {"id":3,"name":"Third status ends with FULL-VALUE-END"},
        {"id":4,"name":"Fourth status"}, {"id":5,"name":"Fifth status"}
    ]).to_string()});
    let h = Harness::new(routes);
    settings(&h, "B");
    let result = scripted(
        &h,
        json!([
            {"wait":"Selected project issue","keys":"l/"},
            {"wait":"Search","resize":[12,40]},
            {"wait":"Search","keys":"\t"},
            {"wait":"[ ] First status","keys":" j j "},
            {"wait":"[x] Third status","keys":"j"},
            {"wait":"[ ] Fourth status","keys":"\r"},
            {"wait":"Search","keys":"\u{1b}[C".repeat(110)},
            {"wait":"FULL-VALUE-END","keys":"\u{b}日本語qhljk\r"},
            {"wait":"> Issue List","keys":"/\t"},
            {"wait":"[x] First status","keys":"jj"},
            {"wait":"[x] Third status","keys":"\r"},
            {"wait":"Search","keys":"\u{1b}"},
            {"wait":"> Issue List","keys":"q"}
        ]),
    );
    let snapshots = result["snapshots"].as_array().unwrap();
    assert!(
        snapshots[4].as_str().unwrap().contains("First status"),
        "movement inside viewport keeps its first row: {result}"
    );
    assert!(
        !snapshots[5].as_str().unwrap().contains("First status"),
        "crossing bottom scrolls by one row: {result}"
    );
    assert!(
        snapshots[5].as_str().unwrap().contains("Second status"),
        "{result}"
    );
    let searches: Vec<_> = browse_requests(&h)
        .into_iter()
        .filter(|r| r["path"].as_str().unwrap().starts_with("/api/v2/issues"))
        .collect();
    assert_eq!(searches.len(), 4);
    for request in &searches[2..] {
        let url = reqwest::Url::parse(&format!(
            "https://localhost{}",
            request["path"].as_str().unwrap()
        ))
        .unwrap();
        assert_eq!(
            url.query_pairs().find(|(k, _)| k == "keyword").unwrap().1,
            "日本語qhljk"
        );
        assert_eq!(
            url.query_pairs()
                .filter(|(k, _)| k == "statusId[]")
                .map(|(_, v)| v.into_owned())
                .collect::<Vec<_>>(),
            vec!["1", "2", "3"]
        );
    }
}

#[test]
fn search_dialog_resize_reveals_milestone_and_preserves_keyword_cursor_through_tiny_size() {
    let mut routes = browsing_routes();
    routes["routes"]["/api/v2/projects/2/versions"] =
        json!({"body":"[{\"id\":10,\"name\":\"Release milestone\"}]"});
    let h = Harness::new(routes);
    settings(&h, "B");
    let result = scripted(
        &h,
        json!([
            {"wait":"Selected project issue","keys":"l/"},
            {"wait":"Search","keys":"日本語qhljk\u{1b}[D\u{1b}[D\u{1b}[D\u{1b}[D\u{1b}[D\u{1b}[Z"},
            {"wait":"[ ] Release milestone","resize":[12,40]},
            {"wait":"マ イ ル ス ト ー ン","resize":[11,39]},
            {"wait":"Terminal too small","keys":"destroy\u{15}\r"},
            {"wait":"Terminal too small","resize":[12,40]},
            {"wait":"マ イ ル ス ト ー ン","keys":"\u{b}X\r"},
            {"wait":"> Issue List","keys":"q"}
        ]),
    );
    assert_eq!(
        result["browse_request_counts"][6], 5,
        "tiny keys must not search: {result}"
    );
    let searches: Vec<_> = h
        .requests()
        .into_iter()
        .filter(|r| {
            r["path"].as_str().unwrap().starts_with("/api/v2/issues?")
                || r["path"]
                    .as_str()
                    .unwrap()
                    .starts_with("/api/v2/issues/count?")
        })
        .collect();
    assert_eq!(searches.len(), 4, "{searches:?}");
    for request in &searches[2..] {
        let url = reqwest::Url::parse(&format!(
            "https://localhost{}",
            request["path"].as_str().unwrap()
        ))
        .unwrap();
        assert_eq!(
            url.query_pairs().find(|(k, _)| k == "keyword").unwrap().1,
            "日本語Xqhljk"
        );
    }
}

#[test]
fn search_dialog_first_open_after_initial_search_failure_can_deselect_default_status() {
    let mut routes = browsing_routes();
    routes["routes"]["/api/v2/projects/2/statuses"] =
        json!({"body":"[{\"id\":1,\"name\":\"open\"}]"});
    routes["routes"]["/api/v2/issues/count"] =
        json!({"sequence":[{"status":403},{"body":"{\"count\":1}"}]});
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
            {"wait":"Failed:","keys":"kl/"},
            {"wait":"open","keys":"\t"},
            {"wait":"[x] open","keys":" "},
            {"wait":"[ ] open","keys":"\r\r"},
            {"wait":"Selected project issue","keys":"q"}
        ]),
    );
    assert!(
        result["snapshots"][4]
            .as_str()
            .unwrap()
            .contains("> Issue List"),
        "{result}"
    );
    let searches: Vec<_> = h
        .requests()
        .into_iter()
        .filter(|r| {
            r["path"].as_str().unwrap().starts_with("/api/v2/issues?")
                || r["path"]
                    .as_str()
                    .unwrap()
                    .starts_with("/api/v2/issues/count?")
        })
        .collect();
    assert_eq!(searches.len(), 3, "{searches:?}");
    assert!(
        searches[0]["path"]
            .as_str()
            .unwrap()
            .contains("statusId%5B%5D=1")
    );
    for request in &searches[1..] {
        assert!(
            !request["path"].as_str().unwrap().contains("statusId"),
            "{request}"
        );
        assert!(
            request["path"]
                .as_str()
                .unwrap()
                .contains("projectId%5B%5D=2"),
            "{request}"
        );
    }
}

#[test]
fn search_dialog_cannot_search_an_unconfirmed_project_highlight() {
    let h = Harness::new(browsing_routes());
    scripted(
        &h,
        json!([
            {"wait":"Select a project","keys":"jl"},
            {"wait":"> Issue List","keys":"/\r"},
            {"wait":"Select a project","keys":"q"}
        ]),
    );
    assert_eq!(h.requests().len(), 2, "only space/projects may be fetched");
}

#[test]
fn search_dialog_candidate_loading_keeps_ambiguous_and_unknown_defaults_as_errors() {
    for (name, error) in [
        ("open", "Multiple candidates match the exact name"),
        ("missing", "No candidate matches the exact name"),
    ] {
        let mut routes = browsing_routes();
        routes["routes"]["/api/v2/projects/2/statuses"] = json!({"body":json!([
            {"id":1,"name":"open"}, {"id":2,"name":"open"}
        ]).to_string()});
        let h = Harness::new(routes);
        settings(&h, "B");
        std::fs::write(
            h.root.join(".backlog/settings.json"),
            json!({"project":"B","issue":{"list":{"status":[name]}}}).to_string(),
        )
        .unwrap();
        let result = scripted(
            &h,
            json!([
                {"wait":error,"keys":"l/\t"},
                {"wait":"[ ] open","keys":"\r\r"},
                {"wait":error,"keys":"\u{1b}"},
                {"wait":"> Issue List","keys":"q"}
            ]),
        );
        assert!(
            result["snapshots"][2].as_str().unwrap().contains(name),
            "{result}"
        );
        assert!(
            !h.requests()
                .iter()
                .any(|r| r["path"].as_str().unwrap().starts_with("/api/v2/issues")),
            "invalid defaults must not be silently removed or resolved to the first duplicate"
        );
    }
}

#[test]
fn search_dialog_after_failed_project_switch_keeps_applied_project_authoritative() {
    let mut routes = browsing_routes();
    routes["routes"]["/api/v2/issues/count"] = json!({"sequence":[
        {"body":"{\"count\":1}"}, {"status":403}, {"body":"{\"count\":1}"}
    ]});
    let h = Harness::new(routes);
    settings(&h, "B");
    scripted(
        &h,
        json!([
            {"wait":"Selected project issue","keys":"k\r"},
            {"wait":"Failed:","keys":"l/"},
            {"wait":"Search","keys":"retained\r"},
            {"wait":"> Issue List","keys":"q"}
        ]),
    );
    let searches: Vec<_> = h
        .requests()
        .into_iter()
        .filter(|r| {
            r["path"].as_str().unwrap().starts_with("/api/v2/issues?")
                || r["path"]
                    .as_str()
                    .unwrap()
                    .starts_with("/api/v2/issues/count?")
        })
        .collect();
    assert_eq!(searches.len(), 5, "{searches:?}");
    assert!(
        searches[2]["path"]
            .as_str()
            .unwrap()
            .contains("projectId%5B%5D=123456789012345678901234567890")
    );
    for request in &searches[3..] {
        let path = request["path"].as_str().unwrap();
        assert!(
            path.contains("projectId%5B%5D=2") && path.contains("keyword=retained"),
            "{request}"
        );
    }
}

#[test]
fn search_dialog_keyword_paste_redacts_whole_value_and_modified_enter_never_applies() {
    let h = Harness::new(browsing_routes());
    settings(&h, "B");
    let keyword = format!(
        "qhljkRc/*{}test-secret\u{1b}]52;c;clipboard\u{7} tail\r\nnext\tend",
        "x".repeat(160)
    );
    let result = scripted(
        &h,
        json!([
            {"wait":"Selected project issue","keys":"l/"},
            {"wait":"Search","resize":[12,40]},
            {"wait":"Search","keys":format!("\u{1b}[200~{keyword}\u{1b}[201~")},
            {"wait":"[REDACTED]","keys":"\u{1b}\r\u{1b}[13;5u"},
            {"wait":"[REDACTED]","resize":[11,39]},
            {"wait":"Terminal too small","keys":"destroy\u{15}\r"},
            {"wait":"Terminal too small","resize":[12,40]},
            {"wait":"[REDACTED]","keys":"\r"},
            {"wait":"> Issue List","keys":"/"},
            {"wait":"[REDACTED]","keys":"\u{1b}"},
            {"wait":"> Issue List","keys":"q"}
        ]),
    );
    let output = result["stdout"].as_str().unwrap();
    assert!(
        !output.contains("test-secret") && !output.contains("\u{1b}]52;c;clipboard"),
        "{result}"
    );
    for index in [3, 4, 7, 9] {
        let screen = result["snapshots"][index].as_str().unwrap();
        assert!(
            !screen.contains("qhljkRc") && !screen.contains("xxxx"),
            "whole input must be hidden: {result}"
        );
    }
    let searches: Vec<_> = browse_requests(&h)
        .into_iter()
        .filter(|r| r["path"].as_str().unwrap().starts_with("/api/v2/issues"))
        .collect();
    assert_eq!(
        searches.len(),
        4,
        "no searches from paste, modifier aliases or tiny keys: {searches:?}"
    );
    let url = reqwest::Url::parse(&format!(
        "https://localhost{}",
        searches[2]["path"].as_str().unwrap()
    ))
    .unwrap();
    assert_eq!(
        url.query_pairs()
            .find(|(key, _)| key == "keyword")
            .unwrap()
            .1,
        keyword.replace("\r\n", " ").replace('\t', " ")
    );
}

#[test]
fn search_dialog_keyword_summary_preserves_ten_graphemes_and_redacts_before_abbreviating() {
    for (keyword, summary) in [
        ("123456789👩‍💻", "123456789👩‍💻"),
        ("123456789👩‍💻尾", "123456789👩‍💻…"),
        ("123456789e\u{301}", "123456789e\u{301}"),
        ("123456789e\u{301}尾", "123456789e\u{301}…"),
        ("日本語検索条件の境界尾", "日本語検索条件の境界…"),
        ("123456789👩‍💻test-secret", "[REDACTED]"),
        ("123456789e\u{301}\u{1b}]52;c;clipboard\u{7}", "[REDACTED]"),
    ] {
        let h = Harness::new(browsing_routes());
        settings(&h, "B");
        let result = scripted(
            &h,
            json!([
                {"wait":"Selected project issue","keys":"l/"},
                {"wait":"Search","keys":format!("\u{1b}[200~{keyword}\u{1b}[201~\r")},
                {"wait":"> Issue List","resize":[12,100]},
                {"wait":"S:0 A:0 G:0","keys":"q"}
            ]),
        );
        // The PTY's simple screen decoder drops combining marks; inspect the full redraw too.
        let output = result["stdout"].as_str().unwrap();
        let redraw = output.rsplit("\u{1b}[2J").next().unwrap();
        let redrawn_summary: String = redraw
            .split("K:")
            .nth(1)
            .unwrap()
            .split("S:0")
            .next()
            .unwrap()
            .split('\u{1b}')
            .map(|part| part.split_once('H').map_or(part, |(_, text)| text))
            .collect();
        assert_eq!(
            redrawn_summary.trim_end(),
            summary,
            "wrong summary: {result}"
        );
        assert!(
            !output.contains("test-secret") && !output.contains("\u{1b}]52;c;clipboard"),
            "secret and control sequences must stay hidden: {result}"
        );
        if summary == "[REDACTED]" {
            assert!(
                !output.contains("123456789"),
                "whole keyword must be hidden: {result}"
            );
        }
        let requests = h.requests();
        assert!(requests.iter().all(|r| r["method"] == "GET"));
        let searches: Vec<_> = requests
            .iter()
            .filter(|r| {
                let path = r["path"].as_str().unwrap();
                path.starts_with("/api/v2/issues?") || path.starts_with("/api/v2/issues/count?")
            })
            .collect();
        assert_eq!(
            searches.len(),
            4,
            "one count/list pair per search: {requests:?}"
        );
        for request in &searches[2..] {
            let url = reqwest::Url::parse(&format!(
                "https://localhost{}",
                request["path"].as_str().unwrap()
            ))
            .unwrap();
            assert_eq!(
                url.query_pairs()
                    .find(|(key, _)| key == "keyword")
                    .unwrap()
                    .1,
                keyword,
                "summary abbreviation must not change either query"
            );
        }
    }
}

#[test]
fn search_dialog_candidate_failure_is_not_empty_success_and_can_be_retried() {
    let mut routes = browsing_routes();
    routes["routes"]["/api/v2/projects/2/statuses"] = json!({"sequence":[
        {"status":403}, {"body":"[{\"id\":1,\"name\":\"Available after retry\"}]"}
    ]});
    let h = Harness::new(routes);
    settings(&h, "B");
    let result = scripted(
        &h,
        json!([
            {"wait":"Selected project issue","keys":"l/\t"},
            {"wait":"Failed:","keys":"HL"},
            {"wait":"Candidates failed","keys":"\r"},
            {"wait":"Available after retry","keys":" \r\r"},
            {"wait":"> Issue List","keys":"q"}
        ]),
    );
    assert!(
        !result["snapshots"][1]
            .as_str()
            .unwrap()
            .contains("No candidates"),
        "{result}"
    );
    let failed = result["snapshots"][2].as_str().unwrap();
    assert!(failed.contains("Enter:retry Esc:"), "{result}");
    assert!(
        !failed.contains("Enter/Esc:"),
        "failure must not advertise Enter as close: {result}"
    );
    let requests = h.requests();
    assert_eq!(
        requests
            .iter()
            .filter(|r| r["path"] == "/api/v2/projects/2/statuses")
            .count(),
        2
    );
    assert!(
        requests
            .iter()
            .any(|r| r["path"].as_str().unwrap().contains("statusId%5B%5D=1"))
    );
}

#[test]
fn search_dialog_ctrl_c_has_priority_over_keyword_and_inflight_apply_and_restores_terminal() {
    for applying in [false, true] {
        let mut routes = browsing_routes();
        routes["routes"]["/api/v2/issues/count"] = json!({"sequence":[{"body":"{\"count\":1}"},{"body":"{\"count\":1}","header_delay":2}]});
        let h = Harness::new(routes);
        settings(&h, "B");
        std::fs::write(h.root.join("shell-steps.json"), json!([
            {"wait":"Selected project issue","keys":if applying { "l/draft\r" } else { "l/draft" }},
            {"wait":if applying { "Applying..." } else { "draft" },"keys":"\u{3}"}
        ]).to_string()).unwrap();
        let result = shell(&h, "scripted", &[]);
        assert_eq!(result["code"], 130, "{result}");
        assert_eq!(result["restored"], true, "{result}");
        assert!(result["elapsed"].as_f64().unwrap() < 2.0, "{result}");
    }
}

#[test]
fn search_dialog_direct_reproduction_fixture_exercises_normal_clear_and_cross_without_writes() {
    let h = Harness::new(serde_json::from_str(include_str!("fixtures/tui_search.json")).unwrap());
    settings(&h, "B");
    std::fs::write(
        h.root.join(".backlog/settings.json"),
        json!({"project":"B","issue":{"list":{"status":["open"]}}}).to_string(),
    )
    .unwrap();
    let result = scripted(
        &h,
        serde_json::from_str(include_str!("fixtures/tui_search_steps.json")).unwrap(),
    );
    assert!(
        result["snapshots"][14]
            .as_str()
            .unwrap()
            .contains("Beta result"),
        "{result}"
    );
    assert!(
        !result["stdout"].as_str().unwrap().contains("test-secret"),
        "{result}"
    );
    for index in [9, 10] {
        assert_search_fields_blank(result["snapshots"][index].as_str().unwrap());
    }
    for (index, focus) in [
        (19, "> Issue Detail"),
        (20, "> Comment"),
        (21, "> Issue List"),
        (22, "> Comment"),
        (23, "> Comment"),
        (24, "> Issue List"),
        (25, "> Comment"),
    ] {
        assert!(
            result["snapshots"][index].as_str().unwrap().contains(focus),
            "{result}"
        );
        assert_eq!(
            result["request_counts"][index], result["request_counts"][18],
            "focus-only GET increase: {result}"
        );
    }
    let requests = h.requests();
    assert!(requests.iter().all(|r| r["method"] == "GET"));
    let searches: Vec<_> = requests
        .iter()
        .filter(|r| {
            r["path"].as_str().unwrap().starts_with("/api/v2/issues?")
                || r["path"]
                    .as_str()
                    .unwrap()
                    .starts_with("/api/v2/issues/count?")
        })
        .collect();
    for request in &searches[4..6] {
        let path = request["path"].as_str().unwrap();
        assert!(
            !path.contains("keyword=")
                && !path.contains("statusId")
                && !path.contains("assigneeId")
                && !path.contains("milestoneId"),
            "{path}"
        );
    }
    assert_eq!(
        requests
            .iter()
            .filter(|r| r["path"]
                .as_str()
                .unwrap()
                .starts_with("/api/v2/issues/count?"))
            .count(),
        5
    );
    assert_eq!(
        requests
            .iter()
            .filter(|r| r["path"].as_str().unwrap().starts_with("/api/v2/issues?"))
            .count(),
        5
    );
}

#[test]
fn search_dialog_space_deselects_repeated_default_status_without_reinserting_it() {
    let mut routes = browsing_routes();
    routes["routes"]["/api/v2/projects/2/statuses"] =
        json!({"body":"[{\"id\":1,\"name\":\"open\"}]"});
    let h = Harness::new(routes);
    settings(&h, "B");
    std::fs::write(
        h.root.join(".backlog/settings.json"),
        json!({"project":"B","issue":{"list":{"status":["open","open"]}}}).to_string(),
    )
    .unwrap();
    let result = scripted(
        &h,
        json!([
            {"wait":"Selected project issue","keys":"l/\t"},
            {"wait":"[x] open","keys":" "},
            {"wait":"[ ] open","keys":"\r\r"},
            {"wait":"> Issue List","keys":"/"},
            {"wait":"キ ー ワ ー ド","keys":"\u{1b}"},
            {"wait":"> Issue List","keys":"q"}
        ]),
    );
    assert_search_fields_blank(result["snapshots"][4].as_str().unwrap());
    let searches: Vec<_> = h
        .requests()
        .into_iter()
        .filter(|r| {
            r["path"].as_str().unwrap().starts_with("/api/v2/issues?")
                || r["path"]
                    .as_str()
                    .unwrap()
                    .starts_with("/api/v2/issues/count?")
        })
        .collect();
    assert_eq!(searches.len(), 4);
    assert!(
        searches[2..]
            .iter()
            .all(|r| !r["path"].as_str().unwrap().contains("statusId"))
    );
}

#[test]
fn search_dialog_keyword_standard_editing_precedes_shortcuts_and_enter_applies_once() {
    let h = Harness::new(browsing_routes());
    settings(&h, "B");
    scripted(
        &h,
        json!([
            {"wait":"Selected project issue","keys":"l/"},
            {"wait":"Search","keys":"abc def\u{1}X\u{5}\u{2}\u{6}\u{17}\u{19}\u{8}\u{b}\u{19}\u{15}\u{19}\u{13}\u{7}"},
            {"wait":"Xabc de","keys":"\r"},
            {"wait":"> Issue List","keys":"q"}
        ]),
    );
    let requests = browse_requests(&h);
    let searches: Vec<_> = requests
        .iter()
        .filter(|r| r["path"].as_str().unwrap().starts_with("/api/v2/issues"))
        .collect();
    assert_eq!(searches.len(), 4, "editing must not apply: {searches:?}");
    for r in &searches[2..] {
        let url = reqwest::Url::parse(&format!("https://localhost{}", r["path"].as_str().unwrap()))
            .unwrap();
        assert_eq!(
            url.query_pairs().find(|(k, _)| k == "keyword").unwrap().1,
            "Xabc de"
        );
    }
    assert!(!h.requests().iter().any(|r| {
        r["path"].as_str().unwrap().contains("/projects/2/statuses")
            || r["path"].as_str().unwrap().contains("/projects/2/versions")
    }));
}

#[test]
fn search_dialog_reuses_success_and_empty_candidates_after_focus_reopen_and_clear_only_within_dialog()
 {
    let mut routes = browsing_routes();
    routes["routes"]["/api/v2/projects/2/statuses"] =
        json!({"body":"[{\"id\":1,\"name\":\"Cached status\"}]","header_delay":0.3});
    routes["routes"]["/api/v2/projects/2/users"] = json!({"body":"[]"});
    routes["routes"]["/api/v2/projects/2/versions"] = json!({"body":"[]"});
    let h = Harness::new(routes);
    settings(&h, "B");
    let result = scripted(
        &h,
        json!([
            {"wait":"Selected project issue","keys":"l/draft\t"},
            {"wait":"Loading candidates","keys":"\u{13}\u{13}"},
            {"wait":"[ ] Cached status","keys":" \r"},
            {"wait":"Cached status (1)","keys":"j"},
            {"wait":"[x] Cached status","keys":"\u{1b}"},
            {"wait":"Cached status (1)","keys":"\t"},
            {"wait":"No candidates","keys":"\t"},
            {"wait":"No candidates","keys":"\t\t"},
            {"wait":"[x] Cached status","keys":"\u{15}"},
            {"wait":"キ ー ワ ー ド","keys":" "},
            {"wait":"[ ] Cached status","keys":"\t"},
            {"wait":"No candidates","keys":"\r"},
            {"wait":"キ ー ワ ー ド","keys":"\u{6}\r"},
            {"wait":"> Issue List","keys":"/\t"},
            {"wait":"[ ] Cached status","keys":"\r"},
            {"wait":"キ ー ワ ー ド","keys":"\u{1b}"},
            {"wait":"> Issue List","keys":"q"}
        ]),
    );
    for index in [9, 12, 15] {
        assert_search_fields_blank(result["snapshots"][index].as_str().unwrap());
    }
    let requests = h.requests();
    for (path, expected) in [("statuses", 2), ("users", 1), ("versions", 1)] {
        assert_eq!(
            requests
                .iter()
                .filter(|r| r["path"] == format!("/api/v2/projects/2/{path}"))
                .count(),
            expected,
            "{requests:?}"
        );
    }
    let searches: Vec<_> = requests
        .iter()
        .filter(|r| {
            r["path"].as_str().unwrap().starts_with("/api/v2/issues?")
                || r["path"]
                    .as_str()
                    .unwrap()
                    .starts_with("/api/v2/issues/count?")
        })
        .collect();
    assert_eq!(
        searches.len(),
        4,
        "Enter popup only closes, Ctrl+F never applies: {searches:?}"
    );
    assert!(
        searches[2..]
            .iter()
            .all(|r| !r["path"].as_str().unwrap().contains("keyword=")
                && !r["path"].as_str().unwrap().contains("statusId"))
    );
}

#[test]
fn cross_project_dialog_compact_return_guidance_and_next_page_keep_results_until_project_search_succeeds()
 {
    let mut routes = browsing_routes();
    routes["routes"]["/api/v2/issues/count"] = json!({"sequence":[
        {"body":"{\"count\":1}"},{"body":"{\"count\":25}"},{"status":403},{"body":"{\"count\":1}"}
    ]});
    routes["routes"]["/api/v2/issues"] = json!({"sequence":[
        {"body":issue_items(1,1)}, {"body":issue_items(2,1)}, {"body":issue_items(3,1)}, {"body":issue_items(4,1),"header_delay":0.5}
    ]});
    let h = Harness::new(routes);
    settings(&h, "B");
    let result = scripted(
        &h,
        json!([
            {"wait":"Issue 01","keys":"\u{1}"},
            {"wait":"Cross-project Search","keys":"cross\r"},
            {"wait":"Issue 02","resize":[12,40]},
            {"wait":"Enter: normal search / create target","keys":"n"},
            {"wait":"Issue 03","keys":"c"},
            {"wait":"Select a concrete project","keys":" "},
            {"wait":"h/l or Tab -> Project","keys":"h\r"},
            {"wait":"Failed:","keys":"l"},
            {"wait":"Issue 03","keys":"h\rl"},
            {"wait":"Loading...","pause":0.1,"keys":""},
            {"wait":"Issue 04","keys":"q"}
        ]),
    );
    assert!(
        result["snapshots"][2]
            .as_str()
            .unwrap()
            .contains("h/l or Tab -> Project"),
        "wide guidance: {result}"
    );
    let notice = result["snapshots"][5].as_str().unwrap();
    assert!(
        notice.contains("h/l or") && notice.contains("Tab -> Project") && notice.contains("Enter"),
        "{result}"
    );
    for index in [8, 9] {
        let screen = result["snapshots"][index].as_str().unwrap();
        assert!(
            screen.contains("Cross-project") && screen.contains("page 2"),
            "retain cross conditions/history until both calls succeed: {result}"
        );
    }
    let searches: Vec<_> = browse_requests(&h)
        .into_iter()
        .filter(|r| r["path"].as_str().unwrap().starts_with("/api/v2/issues"))
        .collect();
    assert_eq!(
        searches.len(),
        8,
        "n is list-only next page, c only guides: {searches:?}"
    );
    assert!(searches[4]["path"].as_str().unwrap().contains("offset=1"));
}

#[test]
fn search_dialog_invalidates_inflight_candidates_on_cache_hit_close_clear_keyword_dialog_close_and_apply()
 {
    for failed in [false, true] {
        for action in ["cache", "close", "clear", "keyword", "dialog", "apply"] {
            let mut routes = browsing_routes();
            routes["routes"]["/api/v2/projects/2/statuses"] =
                json!({"body":"[{\"id\":1,\"name\":\"Cached status\"}]"});
            routes["routes"]["/api/v2/projects/2/users"] = json!({"sequence":[
                if failed {json!({"status":403,"header_delay":0.35})} else {json!({"body":"[{\"id\":7,\"name\":\"STALE USER\"}]","header_delay":0.35})},
                {"body":"[{\"id\":8,\"name\":\"Fresh user\"}]"}
            ]});
            routes["routes"]["/api/v2/projects/2/versions"] =
                json!({"body":"[{\"id\":9,\"name\":\"Current milestone\"}]","header_delay":0.8});
            routes["routes"]["/api/v2/issues/count"] = json!({"sequence":[{"body":"{\"count\":1}"},{"body":"{\"count\":1}","header_delay":0.8}]});
            let h = Harness::new(routes);
            settings(&h, "B");
            let mut steps = vec![
                json!({"wait":"Selected project issue","keys":"l/draft\t"}),
                json!({"wait":"[ ] Cached status","keys":"\t"}),
            ];
            let next = match action {
                "cache" => "\u{13}",
                "close" | "dialog" | "apply" => "\r",
                "clear" => "\u{15}",
                _ => "\u{b}",
            };
            steps.push(json!({"wait":"Loading candidates","keys":next}));
            if action == "cache" {
                steps.extend([
                    json!({"wait":"[ ] Cached status","keys":"\u{7}"}),
                    json!({"wait":"Loading candidates","pause":0.5,"keys":""}),
                    json!({"wait":"[ ] Current milestone","keys":"\u{1}"}),
                ]);
            } else if action == "dialog" {
                steps.extend([
                    json!({"wait":"Search","keys":"\u{1b}"}),
                    json!({"wait":"> Issue List","pause":0.5,"keys":"/\t"}),
                    json!({"wait":"[ ] Cached status","keys":"\u{1}"}),
                ]);
            } else if action == "apply" {
                steps.extend([
                    json!({"wait":"Search","keys":"\r"}),
                    json!({"wait":"Applying...","pause":0.5,"keys":""}),
                    json!({"wait":"K:draft","keys":"/\t"}),
                    json!({"wait":"[ ] Cached status","keys":"\u{1}"}),
                ]);
            } else if action == "keyword" {
                steps.extend([
                    json!({"wait":"Search","pause":0.5,"keys":"\t"}),
                    json!({"wait":"[ ] Cached status","keys":"\t"}),
                ]);
            } else {
                steps.push(json!({"wait":"Search","pause":0.5,"keys":"j"}));
            }
            steps.extend([
                json!({"wait":"[ ] Fresh user","keys":"\r"}),
                json!({"wait":"Search","keys":"\u{1b}"}),
                json!({"wait":"> Issue List","keys":"q"}),
            ]);
            let result = scripted(&h, Value::Array(steps));
            for screen in result["snapshots"].as_array().unwrap() {
                let text = screen.as_str().unwrap();
                assert!(
                    !text.contains("STALE USER") && !text.contains("Failed:"),
                    "{action}/{failed}: {result}"
                );
            }
            if action == "cache" || action == "apply" {
                let busy = result["snapshots"][4].as_str().unwrap();
                assert!(
                    busy.contains(if action == "cache" {
                        "Loading candidates"
                    } else {
                        "Applying..."
                    }),
                    "old response must not clear new busy: {result}"
                );
            }
            let requests = h.requests();
            assert_eq!(
                requests
                    .iter()
                    .filter(|r| r["path"] == "/api/v2/projects/2/users")
                    .count(),
                2,
                "old results are not cached: {action}/{failed}: {requests:?}"
            );
            assert_eq!(
                requests
                    .iter()
                    .filter(|r| r["path"] == "/api/v2/projects/2/statuses")
                    .count(),
                if action == "dialog" || action == "apply" {
                    2
                } else {
                    1
                },
                "{action}/{failed}: {requests:?}"
            );
        }
    }
}

#[test]
fn region_keys_preserve_selection_history_scroll_and_inflight_detail_reload_without_extra_gets() {
    let mut routes = detail_routes();
    let old = (1..=30)
        .map(|n| format!("Retained line {n:02}\n"))
        .collect::<String>();
    routes["routes"]["/api/v2/issues/B-1"] = json!({"sequence":[
        {"body":detail_issue("B-1", &old).to_string()},
        {"body":detail_issue("B-1", "Reload completed after region moves").to_string(),"header_delay":0.8}
    ]});
    let h = Harness::new(routes);
    settings(&h, "B");
    let result = scripted(
        &h,
        json!([
            {"wait":"TITLE: Detail B-1","keys":"LGl"},
            {"wait":"> Comment","keys":"H"},
            {"wait":"> Project List","keys":"L"},
            {"wait":"> Comment","keys":"h"},
            {"wait":"Retained line 30","keys":"H"},
            {"wait":"> Project List","keys":"l"},
            {"wait":"> Issue List","keys":"L"},
            {"wait":"Retained line 30","keys":"R"},
            {"wait":"Loading... Esc: cancel","pause":0.02,"keys":"H"},
            {"wait":"> Issue List","pause":0.02,"keys":"L"},
            {"wait":"> Issue Detail","pause":0.02,"keys":"LH"},
            {"wait":"> Issue List","pause":0.02,"keys":"L"},
            {"wait":"TITLE: Detail B-1","keys":"G"},
            {"wait":"Reload completed after region moves","keys":"H"},
            {"wait":"> Issue List","keys":"L"},
            {"wait":"Reload completed after region moves","keys":"q"}
        ]),
    );
    for index in 0..=7 {
        assert_eq!(
            result["request_counts"][index], 7,
            "settled focus must not add any GET: {result}"
        );
    }
    for index in 8..=11 {
        assert_eq!(
            result["request_counts"][index], 8,
            "focus must not duplicate/cancel reload: {result}"
        );
        assert!(
            result["snapshots"][index]
                .as_str()
                .unwrap()
                .contains("Loading... Esc: cancel"),
            "{result}"
        );
    }
    for index in 12..=15 {
        assert_eq!(
            result["request_counts"][index], 9,
            "completed focus must not add any GET: {result}"
        );
    }
    for index in [1, 2, 3, 4, 5, 6, 7] {
        assert!(
            result["snapshots"][index]
                .as_str()
                .unwrap()
                .contains("Retained line 30"),
            "scroll must survive: {result}"
        );
    }
    for snapshot in result["snapshots"].as_array().unwrap() {
        let text = snapshot.as_str().unwrap();
        assert!(
            text.contains("B-1") && text.contains("total 3 | page 1"),
            "selection/history must survive: {result}"
        );
        assert!(
            !text.contains("Cancelled") && !text.contains("cancelled"),
            "{result}"
        );
    }
    assert_eq!(h.requests().len(), 9, "{result}");
}

#[test]
fn region_memory_survives_dialog_cancel_failure_success_project_switch_and_resize() {
    let mut routes = detail_routes();
    routes["routes"]["/api/v2/issues/count"] = json!({"sequence":[
        {"body":"{\"count\":3}"}, {"status":403},
        {"body":"{\"count\":3}","header_delay":0.3}, {"body":"{\"count\":3}"},
        {"body":"{\"count\":3}"}
    ]});
    routes["routes"]["/api/v2/issues"] = json!({"body":issue_items(1,3)});
    let h = Harness::new(routes);
    settings(&h, "B");
    let result = scripted(
        &h,
        json!([
            {"wait":"TITLE: Detail B-1","keys":"LlH\u{1}"},
            {"wait":"Cross-project Search","keys":"HL"},
            {"wait":"HL","keys":"\tHL"},
            {"wait":"自 分","keys":"\u{1b}"},
            {"wait":"Enter:検","keys":"\u{1b}"},
            {"wait":"> Project List","keys":"L"},
            {"wait":"> Comment","keys":"H\u{1}"},
            {"wait":"Cross-project Search","keys":"HL\r"},
            {"wait":"Failed:","keys":"\u{1b}"},
            {"wait":"> Project List","keys":"L"},
            {"wait":"> Comment","keys":"H\u{1}"},
            {"wait":"Cross-project Search","keys":"HL\r"},
            {"wait":"Applying...","pause":0.01,"keys":"HL\t"},
            {"wait":"> Issue List","keys":"L"},
            {"wait":"> Comment","keys":"H"},
            {"wait":"> Issue List","keys":"h\r"},
            {"wait":"total 3","pause":0.3,"keys":"L"},
            {"wait":"> Comment","resize":[12,40],"keys":"H"},
            {"wait":"> Project List","keys":"L"},
            {"wait":"> Comment","resize":[11,39],"keys":""},
            {"wait":"40x12","keys":"HL"},
            {"wait":"40x12","resize":[24,100]},
            {"wait":"> Comment","keys":"H"},
            {"wait":"> Project List","keys":"l/"},
            {"wait":"Search","keys":"normalHL\r"},
            {"wait":"> Issue List","keys":"L"},
            {"wait":"> Comment","keys":"H"},
            {"wait":"> Issue List","keys":"hk\r"},
            {"wait":"Project: A","keys":"L"},
            {"wait":"> Comment","keys":"H"},
            {"wait":"> Project List","keys":"q"}
        ]),
    );
    for index in [1, 2, 3, 4, 7, 8, 11, 12] {
        let text = result["snapshots"][index].as_str().unwrap();
        assert!(
            text.contains("> Project Lis"),
            "dialog must not change background focus: {result}"
        );
    }
    for index in [5, 9, 16, 18, 23] {
        assert!(
            result["snapshots"][index]
                .as_str()
                .unwrap()
                .contains("> Project List"),
            "{result}"
        );
    }
    assert!(
        result["snapshots"][13]
            .as_str()
            .unwrap()
            .contains("Cross-project"),
        "{result}"
    );
    let requests = h.requests();
    let searches: Vec<_> = requests
        .iter()
        .filter(|r| {
            r["path"]
                .as_str()
                .unwrap()
                .starts_with("/api/v2/issues/count?")
        })
        .collect();
    assert_eq!(searches.len(), 6, "{requests:?}");
    assert!(
        searches[1..3]
            .iter()
            .all(|r| r["path"].as_str().unwrap().contains("keyword=HL")),
        "{requests:?}"
    );
    assert!(
        searches[4]["path"]
            .as_str()
            .unwrap()
            .contains("keyword=normalHL"),
        "{requests:?}"
    );
}

#[test]
fn region_keys_work_during_startup_and_empty_projects_and_ctrl_c_still_restores() {
    for interrupt in [false, true] {
        let h = Harness::new(json!({"routes":{
            "/api/v2/space":{"body":"{\"name\":\"Actual space\"}","header_delay":0.4},
            "/api/v2/projects":{"body":"[]"}
        }}));
        std::fs::write(
            h.root.join("shell-steps.json"),
            json!([
                {"wait":"> Project List","pause":0.01,"keys":"L"},
                {"wait":"> Issue Detail","pause":0.01,"keys":"lH"},
                {"wait":"> Project List","pause":0.01,"keys":"L"},
                {"wait":"> Comment","pause":0.01,"keys":if interrupt { "\u{3}" } else { "H" }},
                {"wait":"No projects","keys":"L"},
                {"wait":"> Comment","keys":"q"}
            ])
            .to_string(),
        )
        .unwrap();
        let result = shell(&h, "scripted", &[]);
        assert_eq!(result["code"], if interrupt { 130 } else { 0 }, "{result}");
        assert_eq!(result["restored"], true, "{result}");
        assert_eq!(
            h.requests().len(),
            if interrupt { 1 } else { 2 },
            "{result}"
        );
    }
}

fn comment_item(id: u128, content: Value, changes: Value) -> Value {
    json!({"id":id.to_string().parse::<serde_json::Number>().unwrap(),"content":content,
        "changeLog":changes,"createdUser":{"id":7,"name":"Comment author"},
        "created":"2026-03-01T00:00:00Z","updated":"2026-03-02T00:00:00Z"})
}

#[test]
fn comment_initial_preview_shows_latest_null_history_with_no_formatting_get() {
    let mut routes = detail_routes();
    routes["routes"]["/api/v2/issues/B-1/comments"] = json!({"commentItems":[
        comment_item(1,json!(null),json!([])),
        comment_item(999999999999999999999999999999,json!(null),json!([
            {"field":"limitDate","originalValue":null,"newValue":"2026-04-01"},
            {"field":"assigner","originalValue":"old","newValue":"new"},
            {"field":"futureField","originalValue":{"nested":[true,null]},"newValue":999999999999999999999999999999_u128.to_string().parse::<serde_json::Number>().unwrap()}
        ]))
    ]});
    routes["routes"]["/api/v2/issues/B-1/comments"]["commentItems"][1]["createdUser"]["name"] =
        json!("Comment author test-secret\u{1b}]52;c;bad\u{7}");
    let h = Harness::new(routes);
    settings(&h, "B");
    let result = scripted(
        &h,
        json!([
            {"wait":"Issue 01","keys":"l\rl","resize":[20,60]},
            {"wait":"futureField","keys":"q"}
        ]),
    );
    let screen = result["screen"].as_str().unwrap();
    for expected in [
        "Comment author",
        "2026/03/01 09:00:00",
        "DUE DATE",
        "ASSIGNEE",
        "futureField",
        "999999999999999999999999999999",
    ] {
        assert!(screen.contains(expected), "missing {expected}: {result}");
    }
    let output = result["stdout"].as_str().unwrap();
    assert!(!output.contains("test-secret") && !output.contains("\x1b]52"));
    assert!(output.contains("[REDACTED]"));
    let requests = h.requests();
    let comments: Vec<_> = requests
        .iter()
        .filter(|r| r["path"].as_str().unwrap().contains("/comments"))
        .collect();
    assert_eq!(comments.len(), 1, "{requests:?}");
    assert!(
        comments[0]["path"]
            .as_str()
            .unwrap()
            .contains("count=20&order=desc")
    );
    assert_eq!(
        requests
            .iter()
            .filter(|r| r["path"] == "/api/v2/issues/B-1")
            .count(),
        1,
        "null history must not fetch formatting issue"
    );
    assert_eq!(
        requests
            .iter()
            .filter(|r| r["path"] == "/api/v2/projects/2")
            .count(),
        1
    );
}

#[test]
fn comment_history_has_visible_field_value_separators_without_metadata() {
    let mut routes = detail_routes();
    routes["routes"]["/api/v2/issues/B-1/comments"] = json!({"commentItems":[
        comment_item(1, Value::Null, json!([
            {"field":"limitDate","originalValue":null,"newValue":"2026-04-01"},
            {"field":"assigner","originalValue":"old","newValue":"new"},
            {"field":"foo","originalValue":"bar | kept","newValue":"baz"},
            {"field":"foob","originalValue":"ar","newValue":"az"}
        ]))
    ]});
    let h = Harness::new(routes);
    settings(&h, "B");
    let result = scripted(
        &h,
        json!([
            {"wait":"Issue 01","keys":"l\rl","resize":[24,80]},
            {"wait":"foob: ar -> az","keys":"q"}
        ]),
    );
    let screen = result["snapshots"][1].as_str().unwrap();
    assert!(screen.contains("╔> Comment═"), "{screen}");
    for expected in [
        "DUE DATE: 未 設 定  -> 2026-04-01",
        "ASSIGNEE: old -> new",
        "foo: bar | kept -> baz",
        "foob: ar -> az",
        "Comment author",
        "2026/03/01 09:00:00",
    ] {
        assert!(screen.contains(expected), "missing {expected}: {screen}");
    }
}

#[test]
fn comment_pages_use_exclusive_huge_id_boundaries_and_oldest_is_one_direct_ascending_get() {
    let base = 999999999999999999999999999000_u128;
    let mut routes = detail_routes();
    routes["routes"]["/api/v2/issues/B-1/comments"] = json!({"commentItems":(1..=45).map(|n|
        comment_item(base+n,json!(null),json!([{"field":"history","originalValue":null,"newValue":format!("History {n:02}")}]))
    ).collect::<Vec<_>>()});
    let h = Harness::new(routes);
    settings(&h, "B");
    let result = scripted(
        &h,
        json!([
            {"wait":"Issue 01","keys":"l\rl","resize":[20,60]},
            {"wait":"History 45","keys":"n".repeat(20)},
            {"wait":"History 25","keys":"n".repeat(20)},
            {"wait":"History 05","keys":"n".repeat(5)},
            {"wait":"No more comments","keys":"p"},
            {"wait":"History 02","keys":"g"},
            {"wait":"History 45","keys":"G"},
            {"wait":"History 01","keys":"p".repeat(20)},
            {"wait":"History 21","keys":"p".repeat(20)},
            {"wait":"History 41","keys":"p".repeat(5)},
            {"wait":"No more comments","keys":"p"},
            {"wait":"No more comments","keys":"q"}
        ]),
    );
    let requests = h.requests();
    let paths: Vec<_> = requests
        .iter()
        .filter_map(|r| r["path"].as_str().filter(|p| p.contains("/comments")))
        .collect();
    let prefix = "/api/v2/issues/B-1/comments?count=20&order=";
    assert_eq!(
        paths,
        vec![
            format!("{prefix}desc"),
            format!("{prefix}desc&maxId={}", base + 26),
            format!("{prefix}desc&maxId={}", base + 6),
            format!("{prefix}desc&maxId={}", base + 1),
            format!("{prefix}desc"),
            format!("{prefix}asc"),
            format!("{prefix}asc&minId={}", base + 20),
            format!("{prefix}asc&minId={}", base + 40),
            format!("{prefix}asc&minId={}", base + 45)
        ]
    );
    for index in [4, 10, 11] {
        let screen = result["snapshots"][index].as_str().unwrap();
        assert!(screen.contains("╔> Comment No more comments═"), "{screen}");
        assert!(!screen.contains("Comment |"), "{screen}");
    }
    // Latest/oldest are comment endpoints, never document Home/End; repeated empty end is local.
    assert_eq!(result["request_counts"][5], result["request_counts"][4]);
    assert_eq!(
        result["request_counts"][7].as_u64().unwrap(),
        result["request_counts"][6].as_u64().unwrap() + 1
    );
    assert_eq!(result["request_counts"][11], result["request_counts"][10]);
    assert!(requests.iter().all(|r| r["method"] == "GET"));
    assert_eq!(browse_requests(&h).len(), 4);
}

#[test]
fn comment_reload_failure_keeps_selection_and_body_scroll_then_success_replaces_only_comment() {
    let h = Harness::new(serde_json::from_str(include_str!("fixtures/tui_comment.json")).unwrap());
    settings(&h, "B");
    let result = scripted(
        &h,
        serde_json::from_str(include_str!("fixtures/tui_comment_steps.json")).unwrap(),
    );
    for index in [3, 5] {
        let screen = result["snapshots"][index].as_str().unwrap();
        assert!(
            screen.contains("Comment 1 line 16") && !screen.contains("Comment 1 line 01"),
            "{result}"
        );
    }
    let fresh = result["snapshots"][6].as_str().unwrap();
    for expected in ["Comment author", "2026/03/01 09:00:00", "Fresh comment"] {
        assert!(fresh.contains(expected), "{expected}: {result}");
    }
    let history = result["snapshots"][7].as_str().unwrap();
    for expected in [
        "DUE DATE",
        "ASSIGNEE",
        "futureField",
        "nested",
        "999999999999999999999999999999",
    ] {
        assert!(history.contains(expected), "{expected}: {result}");
    }
    assert!(
        result["snapshots"][9]
            .as_str()
            .unwrap()
            .contains("futureField"),
        "focus preserves comment scroll: {result}"
    );
    assert_eq!(
        result["request_counts"],
        json!([4, 9, 9, 9, 10, 10, 13, 13, 13, 13])
    );
    assert_eq!(browse_requests(&h).len(), 4);
    let output = result["stdout"].as_str().unwrap();
    assert!(!output.contains("test-secret") && !output.contains("\x1b]52"));
    assert!(output.contains("[REDACTED]"));
    let requests = h.requests();
    assert_eq!(
        requests
            .iter()
            .filter(|r| r["path"].as_str().unwrap().contains("/comments"))
            .count(),
        3
    );
    assert!(requests.iter().all(|r| r["method"] == "GET"));
}

#[test]
fn comment_full_layout_fixed_header_and_compact_body_tail_are_reachable() {
    let h = Harness::new(serde_json::from_str(include_str!("fixtures/tui_comment.json")).unwrap());
    settings(&h, "B");
    let result = scripted(
        &h,
        json!([
            {"wait":"Comment 2 line 01","keys":"L"},
            {"wait":"> Issue Detail","keys":"l"},
            {"wait":"> Comment","resize":[20,60]},
            {"wait":"Comment 2 line 01","keys":"\u{1b}[F"},
            {"wait":"Comment 2 line 30","keys":"q"}
        ]),
    );
    for index in [0, 3, 4] {
        let screen = result["snapshots"][index].as_str().unwrap();
        for text in ["Comment author", "2026/03/01 09:00:00"] {
            assert!(screen.contains(text), "{text}: {result}");
        }
        let comment = if index == 0 {
            screen
                .lines()
                .skip(17)
                .take(6)
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            screen.to_owned()
        };
        for removed in [
            "COMMENT",
            "CHANGE HISTORY",
            "#2",
            "AUTHOR:",
            "CREATED:",
            "UPDATED:",
            "CHANGES",
        ] {
            assert!(!comment.contains(removed), "{removed}: {result}");
        }
    }
    for (index, body) in [(3, "Comment 2 line 01"), (4, "Comment 2 line 30")] {
        let screen = result["snapshots"][index].as_str().unwrap();
        assert!(
            screen.contains("> Comment") && screen.contains(body),
            "{result}"
        );
        assert!(!screen.contains("Issue Detail"), "{result}");
    }
    assert_eq!(result["request_counts"][0], result["request_counts"][4]);
}

#[test]
fn comment_selection_clears_old_document_and_late_initial_response_cannot_clear_new_busy() {
    for old_status in [200, 500] {
        let mut routes = detail_routes();
        routes["routes"]["/api/v2/issues/B-1/comments"] = json!({"commentItems":[comment_item(1,json!(null),json!([{"field":"initial","originalValue":null,"newValue":"INITIAL COMMENT"}]))]});
        routes["routes"]["/api/v2/issues/B-2/comments"] = json!({"commentItems":[comment_item(2,json!(null),json!([{"field":"old","originalValue":null,"newValue":"OLD COMMENT MUST NOT APPEAR"}]))],"header_delay":0.5,"status":old_status});
        routes["routes"]["/api/v2/issues/B-3/comments"] = json!({"status":500,"header_delay":0.9});
        let h = Harness::new(routes);
        settings(&h, "B");
        let result = scripted(
            &h,
            json!([
                {"wait":"Issue 01","keys":"l\rl","resize":[20,60]},
                {"wait":"INITIAL COMMENT","keys":"HjL"},
                {"wait":"B-2","pause":0.2,"keys":"HjL"},
                {"wait":"B-3","pause":0.6,"keys":""},
                {"wait":"Comment preview failed","keys":"q"}
            ]),
        );
        for index in [2, 3, 4] {
            assert!(
                !result["snapshots"][index]
                    .as_str()
                    .unwrap()
                    .contains("INITIAL COMMENT"),
                "selection must clear old document immediately: {result}"
            );
        }
        let pending = result["snapshots"][3].as_str().unwrap();
        assert!(
            pending.contains("B-3") && pending.contains("Loading... Esc: cancel"),
            "{result}"
        );
        assert!(
            !result["stdout"]
                .as_str()
                .unwrap()
                .contains("OLD COMMENT MUST NOT APPEAR"),
            "{result}"
        );
        assert!(
            !pending.contains("Failed:"),
            "old error must not affect new selection: {result}"
        );
        let requests = h.requests();
        assert_eq!(
            requests
                .iter()
                .filter(|r| r["path"].as_str().unwrap().contains("/comments"))
                .count(),
            3
        );
    }
}

#[test]
fn comment_superseded_reload_and_escape_keep_current_document_and_new_busy_without_focus_gets() {
    let mut routes = detail_routes();
    let item = |id, text| {
        comment_item(
            id,
            json!(null),
            json!([{"field":"history","originalValue":null,"newValue":text}]),
        )
    };
    routes["routes"]["/api/v2/issues/B-1/comments"] = json!({"sequence":[
        {"commentItems":[item(1,"Retained comment")]},
        {"status":500,"header_delay":0.4},
        {"commentItems":[item(2,"Fresh comment")],"header_delay":0.9},
        {"commentItems":[item(3,"Cancelled comment MUST NOT APPEAR")],"header_delay":0.6}
    ]});
    let h = Harness::new(routes);
    settings(&h, "B");
    let result = scripted(
        &h,
        json!([
            {"wait":"Issue 01","keys":"l\rl","resize":[20,60]},
            {"wait":"Retained comment","keys":"R"},
            {"wait":"Loading... Esc: cancel","keys":"R"},
            {"wait":"Loading... Esc: cancel","pause":0.5,"keys":""},
            {"wait":"Fresh comment","keys":"R"},
            {"wait":"Loading... Esc: cancel","keys":"\u{1b}"},
            {"wait":"> Issue List","keys":"L"},
            {"wait":"Fresh comment","pause":0.7,"keys":"h"},
            {"wait":"> Issue Detail","keys":"l"},
            {"wait":"Fresh comment","keys":"q"}
        ]),
    );
    let pending = result["snapshots"][3].as_str().unwrap();
    assert!(
        pending.contains("Retained comment")
            && pending.contains("Loading... Esc: cancel")
            && !pending.contains("Failed:"),
        "{result}"
    );
    assert!(
        !result["stdout"]
            .as_str()
            .unwrap()
            .contains("Cancelled comment MUST NOT APPEAR"),
        "{result}"
    );
    for i in [7, 9] {
        let screen = result["snapshots"][i].as_str().unwrap();
        assert!(
            screen.contains("Fresh comment") && !screen.contains("Loading... Esc: cancel"),
            "{result}"
        );
    }
    let counts = &result["request_counts"];
    assert_eq!(counts[5], counts[6]);
    assert_eq!(counts[6], counts[9], "Esc/focus must not add any GET");
    assert_eq!(
        h.requests()
            .iter()
            .filter(|r| r["path"].as_str().unwrap().contains("/comments"))
            .count(),
        4
    );
    assert_eq!(browse_requests(&h).len(), 4);
}

#[test]
fn comment_description_inline_and_over_limit_warning_do_not_add_formatting_gets() {
    let mut routes = detail_routes();
    routes["routes"]["/api/v2/issues/B-1/comments"] = json!({"commentItems":[
        comment_item(1,Value::Null,json!([{"field":"description","originalValue":format!("HIDDEN{}", "x".repeat(9995)),"newValue":"x"},{"field":"other","originalValue":null,"newValue":"other visible"}])),
        comment_item(2,Value::Null,json!([{"field":"description","originalValue":"common old\nlast\n","newValue":"common new\nlast\n"}]))
    ]});
    let h = Harness::new(routes);
    settings(&h, "B");
    let result = scripted(
        &h,
        json!([
            {"wait":"Issue 01","keys":"l\rl","resize":[20,60]},
            {"wait":"common [-old-]{+new+}","keys":"n"},
            {"wait":"10,000","keys":"\u{1b}[F"},
            {"wait":"other visible","keys":"q"}
        ]),
    );
    assert!(!result["stdout"].as_str().unwrap().contains("HIDDEN"));
    let requests = h.requests();
    assert_eq!(
        requests
            .iter()
            .filter(|r| r["path"].as_str().unwrap().contains("/comments"))
            .count(),
        1
    );
    assert_eq!(
        requests
            .iter()
            .filter(|r| r["path"] == "/api/v2/issues/B-1")
            .count(),
        1
    );
    assert_eq!(
        requests
            .iter()
            .filter(|r| r["path"] == "/api/v2/projects/2")
            .count(),
        1
    );
    assert_eq!(result["request_counts"][1], result["request_counts"][3]);
    assert_eq!(result["code"], 0);
    assert_eq!(result["restored"], true);
}

#[test]
fn detail_jst_created_updated_do_not_change_start_due_or_body_dates() {
    let mut routes = detail_routes();
    let mut issue = detail_issue("B-1", "Body 2026-12-31T23:59:59Z");
    issue["created"] = json!("2026-12-31T23:59:59.987Z");
    issue["updated"] = json!("2024-03-01T00:30:00+14:00");
    issue["startDate"] = json!("2026-12-31");
    issue["dueDate"] = json!("2027-01-01");
    routes["routes"]["/api/v2/issues/B-1"] = json!({"body":issue.to_string()});
    let h = Harness::new(routes);
    settings(&h, "B");
    let result = scripted(
        &h,
        json!([
            {"wait":"Issue 01","keys":"l\r","resize":[24,80]},
            {"wait":"Body 2026-12-31T23:59:59Z","keys":"q"}
        ]),
    );
    let screen = result["screen"].as_str().unwrap();
    for expected in [
        "CREATED: 2027/01/01 08:59:59",
        "UPDATED: 2024/02/29 19:30:00",
        "START: 2026-12-31",
        "DUE: 2027-01-01",
        "Body 2026-12-31T23:59:59Z",
    ] {
        assert!(screen.contains(expected), "{expected}: {result}");
    }
}

#[test]
fn comment_empty_loading_cancel_error_titles_retain_states_without_generated_pipes() {
    for (width, height) in [(100, 24), (40, 12)] {
        let mut routes = detail_routes();
        routes["routes"]["/api/v2/issues/B-1/comments"] = json!({"sequence":[
            {"commentItems":[]},
            {"commentItems":[],"header_delay":0.5},
            {"status":500,"header_delay":0.5}
        ]});
        let h = Harness::new(routes);
        settings(&h, "B");
        let result = scripted(
            &h,
            json!([
                {"wait":"Issue 01","keys":"l\rl","resize":[height,width]},
                {"wait":"No comments","keys":"R"},
                {"wait":"> Comment Loading... Esc: cancel","keys":"\u{1b}"},
                {"wait":"> Issue List","keys":"L"},
                {"wait":"> Comment Cancelled; display retained","keys":"R"},
                {"wait":"> Comment Loading... Esc: cancel","keys":""},
                {"wait":"Failed:","keys":"\r"},
                {"wait":"╔> Comment═","keys":"q"}
            ]),
        );
        for index in [1, 7] {
            let screen = result["snapshots"][index].as_str().unwrap();
            assert!(
                screen.contains("╔> Comment═") && screen.contains("No comments"),
                "{screen}"
            );
            assert!(
                !screen.contains("Loading...") && !screen.contains("Cancelled;"),
                "{screen}"
            );
        }
        for index in [2, 5] {
            let screen = result["snapshots"][index].as_str().unwrap();
            assert!(
                screen.contains("╔> Comment Loading... Esc: cancel═"),
                "{screen}"
            );
        }
        assert!(
            result["snapshots"][4]
                .as_str()
                .unwrap()
                .contains("╔> Comment Cancelled; display retained═")
        );
        assert!(result["snapshots"][6].as_str().unwrap().contains("Failed:"));
        assert!(
            result["snapshots"]
                .as_array()
                .unwrap()
                .iter()
                .all(|screen| !screen.as_str().unwrap().contains("Comment |"))
        );
        let requests = h.requests();
        assert_eq!(
            requests
                .iter()
                .filter(|r| r["path"].as_str().unwrap().contains("/comments"))
                .count(),
            3
        );
        assert_eq!(requests.len(), 9, "no title-only GETs: {requests:?}");
        assert!(requests.iter().all(|r| r["method"] == "GET"));
    }
}

fn editor_harness(mode: &str) -> Harness {
    let h = Harness::new(json!({"routes": {
        "/api/v2/space": {"body":"{\"name\":\"Actual space\"}"},
        "/api/v2/projects": {"body":"[]"}
    }}));
    h.config(json!({"editor":["/usr/bin/python3", concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/editor.py"), h.root.join("editor-record"), mode],
        "profiles":{"default":{"spaceUrl":h.origin,"apiKeyEnv":"TEST_KEY"}}}));
    h
}

#[test]
fn settings_editor_owns_cooked_input_and_immediately_redraws_after_resize_without_reloading_config()
{
    let h = editor_harness("settings");
    std::fs::write(
        h.root.join("shell-steps.json"),
        json!([
            {"wait":"Actual space", "keys":"L*\r"},
            {"wait":"EDITOR READY", "resize":[12,40], "keys":"qjk\n"},
            {"wait":"Settings editor closed; restart", "keys":"\u{1b}"},
            {"wait":"> Issue Detail", "keys":"q"}
        ])
        .to_string(),
    )
    .unwrap();
    let result = shell(&h, "scripted", &[]);
    assert_eq!(result["code"], 0, "{result}");
    assert_eq!(result["restored"], true, "{result}");
    let record: Value =
        serde_json::from_str(&std::fs::read_to_string(h.root.join("editor-record")).unwrap())
            .unwrap();
    assert_eq!(record["input"], "qjk\n", "TUI stole editor input: {record}");
    assert_eq!(record["cooked"], true);
    assert_eq!(record["echo"], true);
    assert_eq!(
        std::fs::read_to_string(h.root.join("backlog/config.json")).unwrap(),
        "{\"saved\":true}"
    );
    assert!(
        result["screen"]
            .as_str()
            .unwrap()
            .contains("> Issue Detail")
    );
    assert!(result["stderr"].as_str().unwrap().is_empty());
}

#[test]
fn settings_editor_nonzero_keeps_saves_and_running_editor_origin_and_defaults() {
    let h = editor_harness("raw-nonzero");
    std::fs::write(
        h.root.join("shell-steps.json"),
        json!([
            {"wait":"Actual space", "keys":"*\r"},
            {"wait":"EDITOR READY", "keys":"first\n"},
            {"wait":"Editor did not exit successfully", "keys":"\u{1b}"},
            {"wait":"> Project List", "keys":"*\r"},
            {"wait":"EDITOR READY", "keys":"second\n"},
            {"wait":"Editor did not exit successfully", "keys":"\u{1b}"},
            {"wait":"> Project List", "keys":"R"},
            {"wait":"No projects", "pause":0.2, "keys":"q"}
        ])
        .to_string(),
    )
    .unwrap();
    let result = shell(&h, "scripted", &[]);
    assert_eq!(result["code"], 0, "{result}");
    assert_eq!(result["restored"], true, "{result}");
    let record: Value =
        serde_json::from_str(&std::fs::read_to_string(h.root.join("editor-record")).unwrap())
            .unwrap();
    assert_eq!(record["input"], "second\n");
    assert_eq!(
        std::fs::read_to_string(h.root.join("backlog/config.json")).unwrap(),
        "{\"saved\":true}"
    );
    assert_eq!(
        h.requests().len(),
        3,
        "running origin/key changed or refresh failed"
    );
    assert!(h.requests().iter().all(|r| r["method"] == "GET"));
}

#[test]
fn settings_missing_file_or_parent_only_guides_location_without_creating() {
    for parent in [false, true] {
        let h = editor_harness("settings");
        if parent {
            std::fs::create_dir(h.root.join(".backlog")).unwrap();
        }
        std::fs::write(
            h.root.join("shell-steps.json"),
            json!([
                {"wait":"Actual space", "keys":"*j\r"},
                {"wait":"Existing file required (not created)", "keys":"\u{1b}"},
                {"wait":"> Project List", "keys":"q"}
            ])
            .to_string(),
        )
        .unwrap();
        let result = shell(&h, "scripted", &[]);
        assert_eq!(result["code"], 0, "{result}");
        assert_eq!(result["restored"], true);
        assert!(!h.root.join("editor-record").exists());
        assert!(!h.root.join(".backlog/settings.json").exists());
        assert_eq!(h.root.join(".backlog").exists(), parent);
        assert!(
            result["snapshots"][1]
                .as_str()
                .unwrap()
                .contains(".backlog/settings.json"),
            "{result}"
        );
    }
}

#[test]
fn settings_editor_spawn_failure_restores_same_focus_and_is_quittable() {
    use std::os::unix::fs::PermissionsExt;
    let h = editor_harness("settings");
    let program = h.root.join("editor");
    std::fs::write(&program, "#!/usr/bin/python3\n").unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
    h.config(json!({"editor":[program], "profiles":{"default":{"spaceUrl":h.origin,"apiKeyEnv":"TEST_KEY"}}}));
    std::fs::write(
        h.root.join("shell-steps.json"),
        json!([
            {"wait":"Actual space", "remove":program, "keys":"L*\r"},
            {"wait":"Cannot run editor", "keys":"\u{1b}"},
            {"wait":"> Issue Detail", "keys":"q"}
        ])
        .to_string(),
    )
    .unwrap();
    let result = shell(&h, "scripted", &[]);
    assert_eq!(result["code"], 0, "{result}");
    assert_eq!(result["restored"], true, "{result}");
    assert!(
        result["screen"]
            .as_str()
            .unwrap()
            .contains("> Issue Detail")
    );
}

#[test]
fn settings_editor_parent_interrupt_stops_raw_child_and_exits_130_in_original_terminal() {
    let h = editor_harness("raw-interrupt");
    std::fs::write(
        h.root.join("shell-steps.json"),
        json!([
            {"wait":"Actual space", "keys":"*\r"},
            {"wait":"EDITOR READY", "signal":"SIGINT"}
        ])
        .to_string(),
    )
    .unwrap();
    let result = shell(&h, "scripted", &[]);
    assert_eq!(result["code"], 130, "{result}");
    assert_eq!(result["restored"], true, "{result}");
    assert!(result["stderr"].as_str().unwrap().contains("interrupted"));
    let record: Value =
        serde_json::from_str(&std::fs::read_to_string(h.root.join("editor-record")).unwrap())
            .unwrap();
    assert_eq!(record["raw_noecho"], true);
    assert_eq!(result["termios_diff"], json!([]));
    let pid = record["pid"].to_string();
    assert!(
        !Command::new("/bin/kill")
            .args(["-0", &pid])
            .output()
            .unwrap()
            .status
            .success()
    );
}

#[test]
fn settings_editor_restores_raw_child_exit_before_returning_to_the_same_screen() {
    for mode in ["raw-nonzero", "raw-signal", "raw-settings"] {
        let h = editor_harness(mode);
        let original = std::fs::read_to_string(h.root.join("backlog/config.json")).unwrap();
        let notice = if mode == "raw-settings" {
            "Settings editor closed; restart"
        } else {
            "Editor did not exit successfully"
        };
        let result = scripted(
            &h,
            json!([
                {"wait":"Actual space", "keys":"L*\r"},
                {"wait":"EDITOR READY", "resize":[12,40], "keys":"qjk\n"},
                {"wait":notice, "keys":"\u{1b}"},
                {"wait":"> Issue Detail", "keys":"q"}
            ]),
        );
        assert_eq!(result["code"], 0, "{mode}: {result}");
        assert_eq!(result["restored"], true, "{mode}: {result}");
        assert_eq!(result["termios_diff"], json!([]));
        assert_eq!(result["stderr"], "");
        let record: Value =
            serde_json::from_str(&std::fs::read_to_string(h.root.join("editor-record")).unwrap())
                .unwrap();
        assert_eq!(record["cooked"], true);
        assert_eq!(record["echo"], true);
        assert_eq!(record["raw_noecho"], true);
        assert_eq!(record["input"], "qjk\n");
        assert_eq!(
            std::fs::read_to_string(h.root.join("backlog/config.json")).unwrap(),
            if mode == "raw-signal" {
                &original
            } else {
                "{\"saved\":true}"
            }
        );
    }
}

#[test]
fn settings_editor_failure_preserves_selected_issue_page_detail_scroll_and_region_memory() {
    let mut routes = detail_routes();
    let old = (1..=30)
        .map(|n| format!("Retained line {n:02}\n"))
        .collect::<String>();
    routes["routes"]["/api/v2/issues/B-1"] = json!({"body":detail_issue("B-1", &old).to_string()});
    let h = Harness::new(routes);
    settings(&h, "B");
    h.config(json!({"editor":["/usr/bin/python3", concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/editor.py"), h.root.join("editor-record"), "nonzero"],
        "profiles":{"default":{"spaceUrl":h.origin,"apiKeyEnv":"TEST_KEY"}}}));
    let result = scripted(
        &h,
        json!([
            {"wait":"TITLE: Detail B-1", "keys":"LG"},
            {"wait":"Retained line 30", "keys":"*\r"},
            {"wait":"EDITOR READY", "keys":"editor input\n"},
            {"wait":"Editor did not exit successfully", "keys":"\u{1b}"},
            {"wait":"Retained line 30", "keys":"H"},
            {"wait":"> Project List", "keys":"L"},
            {"wait":"Retained line 30", "keys":"q"}
        ]),
    );
    for index in [1, 4, 5, 6] {
        let text = result["snapshots"][index].as_str().unwrap();
        assert!(
            text.contains("Retained line 30")
                && text.contains("B-1")
                && text.contains("total 3 | page 1"),
            "{result}"
        );
        assert_eq!(
            result["request_counts"][index], result["request_counts"][1],
            "editor caused requests: {result}"
        );
    }
    assert!(
        result["snapshots"][6]
            .as_str()
            .unwrap()
            .contains("> Issue Detail")
    );
}
