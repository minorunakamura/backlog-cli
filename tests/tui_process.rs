mod common;
use common::Harness;
use serde_json::{Value, json};
use std::process::Command;

fn shell(h: &Harness, mode: &str, args: &[&str]) -> Value {
    shell_waiting(h, mode, args, "")
}
fn shell_waiting(h: &Harness, mode: &str, args: &[&str], marker: &str) -> Value {
    shell_waiting_with_deadline(h, mode, args, marker, 8)
}
fn shell_waiting_with_deadline(
    h: &Harness,
    mode: &str,
    args: &[&str],
    marker: &str,
    deadline_seconds: u64,
) -> Value {
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
        .env("SHELL_DEADLINE_SECONDS", deadline_seconds.to_string())
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
    scripted_with_deadline(h, steps, 8)
}
fn scripted_with_deadline(h: &Harness, steps: Value, deadline_seconds: u64) -> Value {
    std::fs::write(h.root.join("shell-steps.json"), steps.to_string()).unwrap();
    let result = shell_waiting_with_deadline(h, "scripted", &[], "", deadline_seconds);
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
                {"wait":"B-2","pause":0.19,"keys":"j",
                 "wait_request":{"method":"GET","path":"/api/v2/issues/B-2","ordinal":1}},
                {"wait":"B-3","pause":0.6,"keys":"",
                 "wait_request":{"method":"GET","path":"/api/v2/issues/B-3","ordinal":1}},
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
        let requests = h.requests();
        for path in ["/api/v2/issues/B-2", "/api/v2/issues/B-3"] {
            assert_eq!(
                requests
                    .iter()
                    .filter(|r| r["method"] == "GET" && r["path"] == path)
                    .count(),
                1,
                "{requests:?}"
            );
        }
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
            {"wait":"K:fix qhljk S:2 A:2 G:2","keys":"h\r"},
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

fn create_routes() -> Value {
    json!({"routes": {
        "/api/v2/space":{"body":"{\"name\":\"Actual space\"}"},
        "/api/v2/projects":{"body":"[{\"id\":1,\"projectKey\":\"A\",\"name\":\"Alpha\"}]"},
        "/api/v2/issues/count":{"body":"{\"count\":0}"},
        "/api/v2/issues":{"body":"[]"},
        "/api/v2/projects/1/issueTypes":{"body":"[{\"id\":2,\"name\":\"Task\"}]"},
        "/api/v2/priorities":{"body":"[{\"id\":3,\"name\":\"Normal\"}]"},
        "/api/v2/projects/1/users":{"body":"[{\"id\":4,\"name\":\"Alice\"}]"},
        "/api/v2/projects/1/versions":{"body":"[{\"id\":5,\"name\":\"First\"},{\"id\":6,\"name\":\"Second\"}]"}
    }})
}

#[test]
fn bl_create_is_one_modal_preserving_text_and_only_confirms_changed_discard() {
    let h = Harness::new(create_routes());
    let result = scripted(
        &h,
        json!([
            {"wait":"Alpha", "keys":"\r"},
            {"wait":"total 0", "keys":"lc"},
            {"wait":"Create issue", "keys":"\u{001b}"},
            {"wait":"Issue List", "keys":"c"},
            {"wait":"Create issue", "keys":"qhljkHL"},
            {"wait":"qhljkHL", "keys":"\t\u{001b}"},
            {"wait":"Discard changes?", "keys":"\u{001b}"},
            {"wait":"Create issue", "keys":"\u{0014}"},
            {"wait":"qhljkHL", "keys":"\u{001b}"},
            {"wait":"Discard changes?", "keys":"\r"},
            {"wait":"Issue List", "keys":"q"}
        ]),
    );
    assert_eq!(result["snapshots"].as_array().unwrap().len(), 11);
    assert!(h.requests().iter().all(|r| r["method"] == "GET"));
}

#[test]
fn bl_create_resolves_defaults_clears_assignee_and_posts_once_after_full_confirmation() {
    let mut routes = create_routes();
    routes["routes"]["POST /api/v2/issues"] =
        json!({"status":201,"body":"{\"id\":9,\"issueKey\":\"A-9\"}","delay":0.3});
    let h = Harness::new(routes);
    std::fs::create_dir(h.root.join(".backlog")).unwrap();
    std::fs::write(
        h.root.join(".backlog/settings.json"),
        json!({"issue":{"create":{"issueType":"Task","priority":"Normal","assignee":"Alice"}}})
            .to_string(),
    )
    .unwrap();
    let result = scripted(
        &h,
        json!([
            {"wait":"Alpha", "keys":"\r"},
            {"wait":"total 0", "keys":"lc"},
            {"wait":"Assignee: Alice", "keys":" Raw title \t\u{0001}"},
            {"wait":"Assignee: Alice", "keys":"\u{007f}\t2024-02-29\t2024-03-01\t\r"},
            {"wait":"Milestones candidates", "keys":" j \r"},
            {"wait":"Milestones: First", "keys":"\t\t\r"},
            {"wait":"Confirm create", "keys":"", "pause":0.2},
            {"wait":"Confirm create", "keys":"s"},
            {"wait":"Sending", "keys":"s\r\u{001b}q\u{0014}bad"},
            {"wait":"Created: A-9", "keys":"\r"},
            {"wait":"Issue List", "keys":"q"}
        ]),
    );
    let requests = h.requests();
    let post: Vec<_> = requests.iter().filter(|r| r["method"] == "POST").collect();
    assert_eq!(post.len(), 1, "{requests:?}");
    assert_eq!(
        post[0]["form"],
        json!({"projectId":["1"],"summary":[" Raw title "],"issueTypeId":["2"],"priorityId":["3"],"startDate":["2024-02-29"],"dueDate":["2024-03-01"],"description":[""],"milestoneId[]":["5","6"]})
    );
    assert_eq!(
        result["request_counts"][6], result["request_counts"][5],
        "confirmation must not POST"
    );
    let confirm = result["snapshots"][6].as_str().unwrap();
    for text in [
        "Profile: default",
        "Origin: https://localhost:",
        "Space: Actual space",
        "Project: A",
        "Raw title",
        "Assignee: (cleared)",
        "First (5)",
        "Second (6)",
        "2024-02-29",
        "2024-03-01",
        "Body:",
    ] {
        assert!(confirm.contains(text), "missing {text}: {confirm}");
    }
    assert_eq!(
        requests
            .iter()
            .filter(|r| r["path"]
                .as_str()
                .unwrap()
                .starts_with("/api/v2/issues/count"))
            .count(),
        2,
        "{requests:?}"
    );
}

#[test]
fn bl_create_interrupt_aborts_and_joins_write_preserving_each_observed_outcome() {
    for (phase, response, marker, outcome) in [
        (
            "before",
            json!({"status":201,"body":"{}"}),
            "",
            "not_created",
        ),
        (
            "sent",
            json!({"status":201,"header_delay":20,"body":"{}"}),
            "post-received",
            "unknown",
        ),
        (
            "headers",
            json!({"status":201,"delay":20,"body":"{}"}),
            "post-headers-sent",
            "created",
        ),
    ] {
        for interrupt in ["key", "signal"] {
            let mut routes = create_routes();
            routes["routes"]["POST /api/v2/issues"] = response.clone();
            let h = Harness::new(routes);
            let mut steps = vec![
                json!({"wait":"Alpha", "keys":"\r"}),
                json!({"wait":"total 0", "keys":"lc"}),
                json!({"wait":"Create issue", "keys":"title\t\u{0019}\r\r\u{0010}\r\r\u{0002}\t\r", "pause":0.3}),
                json!({"wait":"Confirm create", "keys": if phase == "before" { "" } else { "s" }}),
            ];
            let mut last = json!({"wait": if phase == "before" { "Confirm create" } else { "Sending" }, "wait_file":marker, "pause":0.15});
            if interrupt == "key" {
                last["keys"] = json!("\u{0003}");
            } else {
                last["signal"] = json!("SIGINT");
            }
            steps.push(last);
            std::fs::write(h.root.join("shell-steps.json"), json!(steps).to_string()).unwrap();
            let result = shell(&h, "scripted", &[]);
            assert_eq!(result["code"], 130, "{phase}/{interrupt}: {result}");
            assert_eq!(result["restored"], true, "{result}");
            assert!(
                result["stderr"].as_str().unwrap().contains(outcome),
                "{phase}/{interrupt}: {result}"
            );
            assert!(result["elapsed"].as_f64().unwrap() < 5.0, "{result}");
            assert_eq!(
                h.requests()
                    .iter()
                    .filter(|r| r["method"] == "POST")
                    .count(),
                usize::from(phase != "before")
            );
        }
    }
}

#[test]
fn bl_create_masks_entire_input_and_keeps_editor_body_dates_and_focus_through_resize() {
    let mut routes = create_routes();
    routes["routes"]["POST /api/v2/issues"] =
        json!({"status":201,"body":"{\"id\":9,\"issueKey\":\"A-9\"}"});
    let h = Harness::new(routes);
    h.config(json!({"editor":["/usr/bin/python3", concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/editor.py"), h.root.join("editor-record"), "body-file"], "profiles":{"default":{"spaceUrl":h.origin,"apiKeyEnv":"TEST_KEY"}}}));
    let body = format!(
        "  raw body\n{}\nLAST BODY LINE test-secret\u{001b}]52;c;unsafe\u{0007}\n",
        "long body value ".repeat(120)
    );
    std::fs::write(h.root.join("editor-body"), &body).unwrap();
    let title = format!("test-secret{}", "PRIVATE-SUFFIX".repeat(20));
    let result = scripted(
        &h,
        json!([
            {"wait":"Alpha", "keys":"\r"},
            {"wait":"total 0", "keys":"lc"},
            {"wait":"Create issue", "pause":0.3, "keys":title},
            {"wait":"[REDACTED]", "keys":"\t\u{0019}\r\r\u{0010}\r\r\u{0001}\t2023-02-29\t2024-03-01\t\u{0002}"},
            {"wait":"Body edit", "pause":0.1, "keys":""},
            {"wait":"Body edit", "keys":"\r"},
            {"wait":"EDITOR READY", "resize":[12,40], "keys":"qhljk\n"},
            {"wait":"Body editor finished", "keys":"\t\r"},
            {"wait":"real YYYY-MM-DD", "keys":"\u{001b}"},
            {"wait":"Discard changes?", "keys":"\u{001b}"},
            {"wait":"Create issue", "keys":"\u{0001}\t\u{0015}2024-02-29\t\t\u{0002}\t\r"},
            {"wait":"Confirm create", "keys":"G"},
            {"wait":"[End of all input]", "resize":[11,39], "keys":"s\r\tbad"},
            {"wait":"Terminal too small", "resize":[24,100], "keys":""},
            {"wait":"[End of all input]", "keys":"s"},
            {"wait":"Created: A-9", "keys":"\r"},
            {"wait":"Issue List", "keys":"q"}
        ]),
    );
    let output = result["stdout"].as_str().unwrap();
    assert!(
        !result["snapshots"][3]
            .as_str()
            .unwrap()
            .contains("PRIVATE-SUFFIX"),
        "whole input must be masked, including its visible suffix: {}",
        result["snapshots"][3]
    );
    assert!(!output.contains("test-secret") && !output.contains("\u{001b}]52"));
    let record: Value =
        serde_json::from_str(&std::fs::read_to_string(h.root.join("editor-record")).unwrap())
            .unwrap();
    assert_eq!(record["original"], "");
    assert_eq!(record["input"], "qhljk\n");
    assert_eq!(record["mode"], 0o600);
    assert_eq!(record["cooked"], true);
    assert!(!std::path::Path::new(record["path"].as_str().unwrap()).exists());
    let requests = h.requests();
    let post: Vec<_> = requests.iter().filter(|r| r["method"] == "POST").collect();
    assert_eq!(post.len(), 1);
    assert_eq!(post[0]["form"]["summary"], json!([title]));
    assert_eq!(post[0]["form"]["description"], json!([body]));
    assert_eq!(post[0]["form"]["startDate"], json!(["2024-02-29"]));
    assert_eq!(
        result["request_counts"][13], result["request_counts"][12],
        "tiny input must not send"
    );
    let confirm_end = result["snapshots"][12].as_str().unwrap();
    assert!(
        confirm_end.contains("LAST BODY LINE [REDACTED]"),
        "{confirm_end}"
    );
}

#[test]
fn bl_create_cancelled_candidate_load_cannot_fill_a_new_confirmed_target() {
    let mut routes = create_routes();
    routes["routes"]["/api/v2/projects"]["body"] = json!(
        "[{\"id\":1,\"projectKey\":\"A\",\"name\":\"Alpha\"},{\"id\":2,\"projectKey\":\"B\",\"name\":\"Beta\"}]"
    );
    routes["routes"]["/api/v2/projects/1/issueTypes"]["header_delay"] = json!(0.6);
    for (path, body) in [
        ("issueTypes", "[{\"id\":22,\"name\":\"Beta task\"}]"),
        ("users", "[]"),
        ("versions", "[]"),
    ] {
        routes["routes"][format!("/api/v2/projects/2/{path}")] = json!({"body":body});
    }
    let h = Harness::new(routes);
    let result = scripted(
        &h,
        json!([
            {"wait":"Alpha", "keys":"\r"},
            {"wait":"total 0", "keys":"lc"},
            {"wait":"Loading create candidates", "keys":"\u{001b}[27uhj\rlc"},
            {"wait":"Project: B", "pause":0.8, "keys":"\t\u{0019}\r"},
            {"wait":"Beta task", "keys":"\u{001b}[27u\u{001b}[27u"},
            {"wait":"Issue List", "keys":"q"}
        ]),
    );
    assert!(
        result["snapshots"][4]
            .as_str()
            .unwrap()
            .contains("Beta task")
    );
    assert!(
        h.requests()
            .iter()
            .any(|r| r["path"] == "/api/v2/projects/2/issueTypes")
    );
}

#[test]
fn bl_new_create_interrupt_does_not_inherit_a_previous_created_outcome() {
    let mut routes = create_routes();
    routes["routes"]["POST /api/v2/issues"] =
        json!({"status":201,"body":"{\"id\":9,\"issueKey\":\"A-9\"}"});
    let h = Harness::new(routes);
    std::fs::write(h.root.join("shell-steps.json"), json!([
        {"wait":"Alpha", "keys":"\r"},
        {"wait":"total 0", "keys":"lc"},
        {"wait":"Create issue", "pause":0.3, "keys":"title\t\u{0019}\r\r\u{0010}\r\r\u{0002}\t\r"},
        {"wait":"Confirm create", "keys":"s"},
        {"wait":"Created; list refreshed", "keys":"\rc\u{0003}"}
    ]).to_string()).unwrap();
    let result = shell(&h, "scripted", &[]);
    assert_eq!(result["code"], 130, "{result}");
    assert_eq!(result["restored"], true, "{result}");
    assert!(
        result["stderr"].as_str().unwrap().contains("not_created"),
        "{result}"
    );
    assert_eq!(
        h.requests()
            .iter()
            .filter(|r| r["method"] == "POST")
            .count(),
        1
    );
}

#[test]
fn bl_create_refresh_swaps_history_only_after_both_reads_and_never_injects_created_issue() {
    for failure in ["none", "count", "list"] {
        let mut routes = create_routes();
        routes["routes"]["POST /api/v2/issues"] =
            json!({"status":201,"body":"{\"id\":9,\"issueKey\":\"A-9\"}"});
        routes["routes"]["/api/v2/issues/count"] = json!({"sequence":[{"body":"{\"count\":40}"}, if failure == "count" { json!({"status":500}) } else { json!({"body":"{\"count\":1}"}) }]});
        routes["routes"]["GET /api/v2/issues"] = json!({"sequence":[{"body":issue_items(1,20)},{"body":issue_items(21,40)}, if failure == "list" { json!({"status":503}) } else { json!({"body":issue_items(99,99)}) }]});
        routes["routes"]["/api/v2/projects/2"] =
            json!({"body":"{\"textFormattingRule\":\"markdown\"}"});
        for n in [1, 21, 38, 99] {
            let key = format!("B-{n}");
            routes["routes"][format!("/api/v2/issues/{key}")] =
                json!({"body":detail_issue(&key, "Preview").to_string()});
            routes["routes"][format!("/api/v2/issues/{key}/comments")] = json!({"body":"[]"});
        }
        let h = Harness::new(routes);
        let result = scripted(
            &h,
            json!([
                {"wait":"Alpha", "keys":"\r"},
                {"wait":"total 40", "keys":"ln"},
                {"wait":"B-21", "keys":"jjjjjjjjjjjjjjjjj"},
                {"wait":"> B-38", "keys":"c"},
                {"wait":"Create issue", "pause":0.3, "keys":"title\t\u{0019}\r\r\u{0010}\r\r\u{0002}\t\r"},
                {"wait":"Confirm create", "keys":"s"},
                {"wait": if failure == "none" { "Created; list refreshed" } else { "Created; list refresh failed" }, "keys":"\r"},
                {"wait": if failure == "none" { "> B-99" } else { "> B-38" }, "keys":"p"},
                {"wait": if failure == "none" { "> B-99" } else { "> B-1" }, "keys":"n"},
                {"wait": if failure == "none" { "> B-99" } else { "> B-38" }, "keys":"q"}
            ]),
        );
        let snapshots = result["snapshots"].as_array().unwrap();
        if failure != "none" {
            let list = |i: usize| {
                snapshots[i]
                    .as_str()
                    .unwrap()
                    .lines()
                    .skip(10)
                    .take(13)
                    .map(|line| line.chars().take(40).collect::<String>())
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                list(3),
                list(7),
                "{failure}: selection/page/scroll must remain"
            );
            assert_eq!(list(3), list(9), "{failure}: page history must remain");
        }
        assert_eq!(
            result["browse_request_counts"][8], result["browse_request_counts"][9],
            "history navigation must be local: {failure}"
        );
        let requests = h.requests();
        let counts: Vec<_> = requests
            .iter()
            .filter(|r| {
                r["path"]
                    .as_str()
                    .unwrap()
                    .starts_with("/api/v2/issues/count")
            })
            .collect();
        assert_eq!(counts.len(), 2);
        assert_eq!(counts[0]["path"], counts[1]["path"]);
        let lists: Vec<_> = requests
            .iter()
            .filter(|r| {
                r["method"] == "GET" && r["path"].as_str().unwrap().starts_with("/api/v2/issues?")
            })
            .collect();
        assert_eq!(
            lists.len(),
            if failure == "count" { 2 } else { 3 },
            "{requests:?}"
        );
        if failure != "count" {
            assert_eq!(lists[0]["path"], lists[2]["path"]);
        }
        assert_eq!(requests.iter().filter(|r| r["method"] == "POST").count(), 1);
        assert!(!snapshots[7].as_str().unwrap().contains("> A-9"));
    }
}

#[test]
fn bl_create_requires_explicit_choices_and_compares_current_values_not_a_dirty_flag() {
    let h = Harness::new(create_routes());
    let result = scripted(
        &h,
        json!([
            {"wait":"Alpha", "keys":"\r"},
            {"wait":"total 0", "keys":"lc"},
            {"wait":"Create issue", "pause":0.3, "keys":"x\t\u{0002}\t\r"},
            {"wait":"Issue type and priority are required", "keys":"\u{0014}\u{0015}\u{001b}"},
            {"wait":"Issue List", "resize":[12,40], "keys":"c"},
            {"wait":"Create issue", "pause":0.3, "keys":"\t"},
            {"wait":"> Issue type", "keys":"\t"},
            {"wait":"> Priority", "keys":"\t"},
            {"wait":"> Assignee", "keys":"\t"},
            {"wait":"> Start date", "keys":"\t"},
            {"wait":"> Due date", "keys":"\t"},
            {"wait":"> Milestones", "keys":"\t"},
            {"wait":"> Body edit", "keys":"\t"},
            {"wait":"> Review", "keys":"\t"},
            {"wait":"> Cancel", "keys":"\u{001b}"},
            {"wait":"Issue List", "keys":"q"}
        ]),
    );
    assert!(
        !result["stdout"]
            .as_str()
            .unwrap()
            .contains("Discard changes?")
    );
    for index in 6..=14 {
        assert!(
            result["snapshots"][index]
                .as_str()
                .unwrap()
                .contains("Tab/S-Tab focus Esc cancel")
        );
    }
    assert!(h.requests().iter().all(|r| r["method"] == "GET"));
}

#[test]
fn bl_create_editor_failures_keep_other_inputs_and_do_not_change_the_empty_body() {
    let h = Harness::new(create_routes());
    h.config(json!({"editor":["/usr/bin/python3", concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/editor.py"), h.root.join("editor-record"), "nonzero"],"profiles":{"default":{"spaceUrl":h.origin,"apiKeyEnv":"TEST_KEY"}}}));
    let result = scripted(
        &h,
        json!([
            {"wait":"Alpha", "keys":"\r"},
            {"wait":"total 0", "keys":"lc"},
            {"wait":"Create issue", "pause":0.3, "keys":"retained title\t\u{0002}\r"},
            {"wait":"EDITOR READY", "resize":[12,40], "keys":"qjk\n"},
            {"wait":"Create issue", "keys":"\u{0014}"},
            {"wait":"retained title", "keys":"\t\u{0002}"},
            {"wait":"Body edit: 0 chars", "keys":"\u{001b}"},
            {"wait":"Discard changes?", "keys":"\r"},
            {"wait":"Issue List", "keys":"q"}
        ]),
    );
    let record: Value =
        serde_json::from_str(&std::fs::read_to_string(h.root.join("editor-record")).unwrap())
            .unwrap();
    assert!(!std::path::Path::new(record["path"].as_str().unwrap()).exists());
    assert_eq!(record["cooked"], true);
    assert_eq!(record["original"], "");
    assert!(result["snapshots"][4].as_str().unwrap().contains("0 chars"));
    assert!(h.requests().iter().all(|r| r["method"] == "GET"));
}

#[test]
fn bl_create_result_does_not_downgrade_success_headers_or_refresh_an_unreadable_created_response() {
    for (response, outcome) in [
        (
            json!({"status":201,"body":"bad json"}),
            "Created; response processing failed",
        ),
        (
            json!({"status":201,"truncate":true,"body":"{}"}),
            "Created; response processing failed",
        ),
        (
            json!({"status":500,"body":"{}"}),
            "Unknown: creation result unknown",
        ),
        (
            json!({"status":400,"body":"{\"errors\":[{\"code\":7,\"message\":\"Rejected\"}]}"}),
            "NotCreated: issue not created",
        ),
    ] {
        let mut routes = create_routes();
        routes["routes"]["POST /api/v2/issues"] = response;
        let h = Harness::new(routes);
        scripted(
            &h,
            json!([
                {"wait":"Alpha", "keys":"\r"},
                {"wait":"total 0", "keys":"lc"},
                {"wait":"Create issue", "pause":0.3, "keys":"title\t\u{0019}\r\r\u{0010}\r\r\u{0002}\t\r"},
                {"wait":"Confirm create", "keys":"\r", "pause":0.15},
                {"wait":"Confirm create", "keys":"s"},
                {"wait":outcome, "keys":"s\r"},
                {"wait":"Issue List", "keys":"q"}
            ]),
        );
        let requests = h.requests();
        assert_eq!(requests.iter().filter(|r| r["method"] == "POST").count(), 1);
        assert_eq!(
            requests
                .iter()
                .filter(|r| r["path"]
                    .as_str()
                    .unwrap()
                    .starts_with("/api/v2/issues/count"))
                .count(),
            1,
            "unusable success response must not GET just to confirm certainty"
        );
    }
}

fn update_issue() -> Value {
    let mut issue = detail_issue("B-1", "Original body");
    issue["issueType"]["id"] = json!(2);
    issue["priority"]["id"] = json!(3);
    issue["status"]["id"] = json!(1);
    issue["milestone"] = json!([{"id":5,"name":"First"}]);
    issue
}
fn update_routes() -> Value {
    let mut routes = browsing_routes();
    let mut latest = update_issue();
    latest["summary"] = json!("Latest title");
    routes["routes"]["GET /api/v2/issues/B-1"] = json!({"sequence":[
        {"body":update_issue().to_string()}, {"body":latest.to_string()}
    ]});
    latest["summary"] = json!("Changed title");
    routes["routes"]["PATCH /api/v2/issues/B-1"] = json!({"body":latest.to_string(),"delay":0.3});
    for (path, body) in [
        (
            "/api/v2/projects/2/issueTypes",
            json!([{"id":2,"name":"Task"}]),
        ),
        ("/api/v2/priorities", json!([{"id":3,"name":"Medium"}])),
        ("/api/v2/projects/2/users", json!([{"id":4,"name":"Alice"}])),
        (
            "/api/v2/projects/2/versions",
            json!([{"id":5,"name":"First"},{"id":6,"name":"Second"}]),
        ),
        (
            "/api/v2/projects/2/statuses",
            json!([{"id":1,"name":"Open"},{"id":2,"name":"Closed"}]),
        ),
        ("/api/v2/resolutions", json!([{"id":0,"name":"Fixed"}])),
    ] {
        routes["routes"][path] = json!({"body":body.to_string()});
    }
    routes
}
#[test]
fn bl_create_and_update_single_line_editing_precedes_form_shortcuts_even_when_edit_is_noop() {
    for update in [false, true] {
        let mut routes = if update {
            update_routes()
        } else {
            create_routes()
        };
        if update {
            let mut latest = update_issue();
            latest["summary"] = json!("");
            latest["startDate"] = json!(null);
            latest["dueDate"] = json!(null);
            routes["routes"]["GET /api/v2/issues/B-1"]["sequence"][1]["body"] =
                json!(latest.to_string());
        } else {
            routes["routes"]["POST /api/v2/issues"] =
                json!({"status":201,"body":"{\"id\":9,\"issueKey\":\"A-9\"}"});
        }
        let h = Harness::new(routes);
        let mut steps = if update {
            settings(&h, "B");
            vec![json!({"wait":"TITLE: Detail B-1", "keys":"Le"})]
        } else {
            vec![
                json!({"wait":"Alpha", "keys":"\r"}),
                json!({"wait":"total 0", "keys":"lc"}),
            ]
        };
        steps.extend([
            json!({"wait":"> Title:", "keys":"\u{1}\u{2}\u{19}\u{1b}[200~日本e\u{301}👩‍💻 qhljkHL\u{1b}[201~\u{1}\u{1}\u{2}X\u{5}\u{2}!\u{b}\u{19}"}),
            json!({"wait":"qhljkH!L", "keys":"\t\t\t\t"}),
            json!({"wait":"> Start date:", "keys":"\u{1}\u{2}\u{19}2024-02-2\u{2}0\u{1}\u{1}\u{2}\u{1b}[3~2\u{5}\u{2}\u{b}\u{19}"}),
            json!({"wait":"> Start date: 2024-02-02", "keys":"\t"}),
            json!({"wait":"> Due date:", "keys":"\u{1}\u{2}\u{19}2024-03-0\u{2}1\u{1}\u{1}\u{2}\u{1b}[3~2\u{5}\u{2}\u{b}\u{19}"}),
            json!({"wait":"> Due date: 2024-03-10", "keys":"\t\u{1}"}),
            json!({"wait":"> Assignee:", "keys":"\u{19}"}),
            json!({"wait":"> Issue type:", "keys":"\r\r\u{10}"}),
            json!({"wait":"> Priority:", "keys":"\r\r\u{14}\u{10}"}),
            json!({"wait":"> Priority:", "keys":"\u{2}"}),
            json!({"wait":"> Body edit:", "keys":if update {"\t\t\t\r"} else {"\t\r"}}),
            json!({"wait":if update {"Confirm update"} else {"Confirm create"}, "keys":"\r"}),
            json!({"wait":if update {"Confirm update"} else {"Confirm create"}, "keys":"s"}),
            json!({"wait":if update {"Comment display refreshed"} else {"Created: A-9"}, "keys":"\r"}),
            json!({"wait":if update {"> Issue Detail"} else {"Issue List"}, "keys":"q"}),
        ]);
        let result = scripted(&h, json!(steps));
        let snapshots = result["snapshots"].as_array().unwrap();
        let confirmation = snapshots[snapshots.len() - 3].as_str().unwrap();
        for text in ["qhljkH!L", "2024-02-02", "2024-03-10"] {
            assert!(confirmation.contains(text), "missing {text}: {result}");
        }
        let counts = result["request_counts"].as_array().unwrap();
        assert_eq!(
            counts[counts.len() - 4],
            counts[counts.len() - 3],
            "Enter must not send: {result}"
        );
        let requests = h.requests();
        let writes: Vec<_> = requests.iter().filter(|r| r["method"] != "GET").collect();
        assert_eq!(writes.len(), 1, "{requests:?}");
        assert_eq!(writes[0]["method"], if update { "PATCH" } else { "POST" });
        assert_eq!(
            writes[0]["form"]["summary"],
            json!(["X日本e\u{301}👩‍💻 qhljkH!L"])
        );
        assert_eq!(writes[0]["form"]["startDate"], json!(["2024-02-02"]));
        assert_eq!(writes[0]["form"]["dueDate"], json!(["2024-03-10"]));
    }
}

#[test]
fn bl_update_fetches_latest_confirms_only_changes_and_refreshes_only_comments_once() {
    let h = Harness::new(update_routes());
    settings(&h, "B");
    let result = scripted(
        &h,
        json!([
            {"wait":"TITLE: Detail B-1", "keys":"L"},
            {"wait":"> Issue Detail", "keys":"e"},
            {"wait":"Latest title", "keys":"\u{15}Changed title\t\u{2}\t\t\t\r"},
            {"wait":"Confirm update", "keys":"\r"},
            {"wait":"Confirm update", "keys":"s"},
            {"wait":"Sending", "keys":"s\r\u{1b}q\u{14}bad"},
            {"wait":"Comment display refreshed", "keys":"\r"},
            {"wait":"Changed title", "keys":"H"},
            {"wait":"Selected project issue", "keys":"q"}
        ]),
    );
    assert_eq!(
        result["request_counts"][3], result["request_counts"][4],
        "confirm Enter must not write: {result}"
    );
    let confirm = result["snapshots"][3].as_str().unwrap();
    assert!(
        confirm.contains("Changed title") && !confirm.contains("Original body"),
        "{result}"
    );
    let requests = h.requests();
    let patches: Vec<_> = requests.iter().filter(|r| r["method"] == "PATCH").collect();
    assert_eq!(patches.len(), 1, "{requests:?}");
    assert_eq!(patches[0]["form"], json!({"summary":["Changed title"]}));
    for prefix in ["/api/v2/issues/count?", "/api/v2/issues?"] {
        assert_eq!(
            requests
                .iter()
                .filter(|r| r["path"].as_str().unwrap().starts_with(prefix))
                .count(),
            1,
            "{requests:?}"
        );
    }
    assert_eq!(
        requests
            .iter()
            .filter(|r| r["path"] == "/api/v2/issues/B-1/comments?count=20&order=desc")
            .count(),
        2,
        "{requests:?}"
    );
    assert_eq!(
        requests
            .iter()
            .filter(|r| r["method"] == "GET" && r["path"] == "/api/v2/issues/B-1")
            .count(),
        2,
        "{requests:?}"
    );
}

#[test]
fn bl_update_required_choices_cannot_be_cleared_and_no_changes_never_patch_or_confirm_discard() {
    let h = Harness::new(update_routes());
    settings(&h, "B");
    let result = scripted(
        &h,
        json!([
            {"wait":"TITLE: Detail B-1", "keys":"Le"},
            {"wait":"Latest title", "keys":"\t\u{7f}\t\u{7f}\u{2}\t\u{7f}\t\t\r"},
            {"wait":"No fields have changed", "keys":"\u{1b}"},
            {"wait":"> Issue Detail", "keys":"q"}
        ]),
    );
    assert!(
        !result["stdout"]
            .as_str()
            .unwrap()
            .contains("Discard changes?"),
        "{result}"
    );
    assert!(h.requests().iter().all(|r| r["method"] == "GET"));
}

#[test]
fn bl_update_all_fields_include_resolution_zero_and_editor_raw_body_in_one_patch() {
    let mut routes = update_routes();
    routes["routes"]["/api/v2/projects/2/issueTypes"]["body"] =
        json!("[{\"id\":2,\"name\":\"Task\"},{\"id\":7,\"name\":\"Bug\"}]");
    routes["routes"]["/api/v2/priorities"]["body"] =
        json!("[{\"id\":3,\"name\":\"Medium\"},{\"id\":8,\"name\":\"High\"}]");
    let h = Harness::new(routes);
    settings(&h, "B");
    // Invalid create defaults must never participate in an update.
    std::fs::write(h.root.join(".backlog/settings.json"), json!({"project":"B","issue":{"create":{"issueType":"MISSING","priority":"MISSING","assignee":"self"}}}).to_string()).unwrap();
    let editor = vec![
        "/usr/bin/python3".to_string(),
        format!("{}/tests/fixtures/editor.py", env!("CARGO_MANIFEST_DIR")),
        h.root.join("editor-record").display().to_string(),
        "body-file".into(),
    ];
    h.config(json!({"editor":editor,"profiles":{"default":{"spaceUrl":h.origin,"apiKeyEnv":"TEST_KEY"}}}));
    let body = " Raw body 日本語 e\u{301} 👩‍💻\n\n";
    std::fs::write(h.root.join("editor-body"), body).unwrap();
    let result = scripted(
        &h,
        json!([
            {"wait":"TITLE: Detail B-1", "keys":"Le"},
            {"wait":"Latest title", "keys":"\u{15}Changed title\t\rj\r\t\rj\r\t\rj\r\t2024-02-29\t2024-03-01\t\r"},
            {"wait":"Milestones candidates", "keys":" j \r\t\r"},
            {"wait":"EDITOR READY", "keys":"\n", "resize":[12,40]},
            {"wait":"Body editor finished", "keys":"\t\rj\r\t\rj\r\t\r", "resize":[24,100]},
            {"wait":"Confirm update", "keys":"G"},
            {"wait":"[End of changes]", "keys":"s"},
            {"wait":"Comment display refreshed", "keys":"\r"},
            {"wait":"> Issue Detail", "keys":"q"}
        ]),
    );
    let requests = h.requests();
    let patches: Vec<_> = requests.iter().filter(|r| r["method"] == "PATCH").collect();
    assert_eq!(patches.len(), 1, "{requests:?}");
    assert_eq!(
        patches[0]["form"],
        json!({"summary":["Changed title"],"description":[body],"issueTypeId":["7"],"priorityId":["8"],"assigneeId":["4"],"statusId":["2"],"resolutionId":["0"],"startDate":["2024-02-29"],"dueDate":["2024-03-01"],"milestoneId[]":["6"]})
    );
    assert!(!requests.iter().any(|r| r["path"] == "/api/v2/users/myself"));
    assert!(
        result["snapshots"][6]
            .as_str()
            .unwrap()
            .contains("Fixed (0)"),
        "{result}"
    );
}

#[test]
fn bl_update_explicit_clears_are_empty_values_and_unchanged_required_fields_are_omitted() {
    let mut routes = update_routes();
    let mut latest = update_issue();
    latest["assignee"] = json!({"id":4,"name":"Alice"});
    latest["resolution"] = json!({"id":0,"name":"Fixed"});
    latest["startDate"] = json!("2024-02-29T00:00:00Z");
    latest["dueDate"] = json!("2024-03-01T00:00:00Z");
    routes["routes"]["GET /api/v2/issues/B-1"]["sequence"][1]["body"] = json!(latest.to_string());
    let h = Harness::new(routes);
    settings(&h, "B");
    let result = scripted(
        &h,
        json!([
            {"wait":"TITLE: Detail B-1","keys":"Le"},
            {"wait":"Assignee: Alice (4)","keys":"\t\t\t\u{7f}\t\u{15}\t\u{15}\t\u{7f}\t\u{7f}\t\t\u{7f}\t\r"},
            {"wait":"Confirm update","keys":"s"},
            {"wait":"Comment display refreshed","keys":"\r"},
            {"wait":"> Issue Detail","keys":"q"}
        ]),
    );
    let confirm = result["snapshots"][2].as_str().unwrap();
    for field in [
        "Assignee",
        "Resolution",
        "Start date",
        "Due date",
        "Milestones",
        "Body",
    ] {
        assert!(confirm.contains(&format!("{field}: (cleared)")), "{result}");
    }
    let patches: Vec<_> = h
        .requests()
        .into_iter()
        .filter(|r| r["method"] == "PATCH")
        .collect();
    assert_eq!(patches.len(), 1);
    assert_eq!(
        patches[0]["form"],
        json!({"description":[""],"assigneeId":[""],"resolutionId":[""],"startDate":[""],"dueDate":[""],"milestoneId[]":[""]})
    );
}

#[test]
fn bl_update_same_ids_and_milestone_sets_are_not_changes_even_with_different_candidate_labels() {
    let mut routes = update_routes();
    let mut latest = update_issue();
    latest["milestone"] = json!([{"id":5,"name":"First"},{"id":6,"name":"Second"}]);
    routes["routes"]["GET /api/v2/issues/B-1"]["sequence"][1]["body"] = json!(latest.to_string());
    routes["routes"]["/api/v2/projects/2/issueTypes"]["body"] =
        json!("[{\"id\":2,\"name\":\"Renamed task\"}]");
    let h = Harness::new(routes);
    settings(&h, "B");
    let result = scripted(
        &h,
        json!([
            {"wait":"TITLE: Detail B-1","keys":"Le"},
            {"wait":"Issue type: Task (2)","keys":"\t\r\r\t\t\t\t\t\r"},
            {"wait":"Milestones candidates","keys":"  \r\t\t\t\t\r"},
            {"wait":"No fields have changed","keys":"\u{1b}"},
            {"wait":"> Issue Detail","keys":"q"}
        ]),
    );
    assert!(
        !result["stdout"]
            .as_str()
            .unwrap()
            .contains("Discard changes?"),
        "{result}"
    );
    assert!(h.requests().iter().all(|r| r["method"] == "GET"));
}

#[test]
fn bl_update_interrupt_aborts_joins_and_keeps_not_updated_unknown_or_updated() {
    for (phase, response, marker, outcome) in [
        ("before", json!({"body":"{}"}), "", "not_updated"),
        (
            "sent",
            json!({"header_delay":20,"body":"{}"}),
            "patch-received",
            "unknown",
        ),
        (
            "headers",
            json!({"delay":20,"body":"{}"}),
            "patch-headers-sent",
            "updated",
        ),
    ] {
        for signal in [false, true] {
            let mut routes = update_routes();
            routes["routes"]["PATCH /api/v2/issues/B-1"] = response.clone();
            let h = Harness::new(routes);
            settings(&h, "B");
            let mut last = json!({"wait":if phase == "before" {"Confirm update"} else {"Sending"}, "wait_file":marker, "pause":0.15});
            if signal {
                last["signal"] = json!("SIGINT");
            } else {
                last["keys"] = json!("\u{3}");
            }
            std::fs::write(
                h.root.join("shell-steps.json"),
                json!([
                    {"wait":"TITLE: Detail B-1","keys":"Le"},
                    {"wait":"Latest title","keys":"!\t\u{2}\t\t\t\r"},
                    {"wait":"Confirm update","keys":if phase == "before" {""} else {"s"}},last
                ])
                .to_string(),
            )
            .unwrap();
            let result = shell(&h, "scripted", &[]);
            assert_eq!(result["code"], 130, "{phase}/{signal}: {result}");
            assert_eq!(result["restored"], true, "{result}");
            let stderr = result["stderr"].as_str().unwrap();
            assert!(
                stderr.contains(&format!("; {outcome}:")),
                "{phase}/{signal}: {result}"
            );
            if phase != "before" {
                assert!(stderr.contains("do not resend"), "{result}");
            }
            assert!(result["elapsed"].as_f64().unwrap() < 5.0, "{result}");
            assert_eq!(
                h.requests()
                    .iter()
                    .filter(|r| r["method"] == "PATCH")
                    .count(),
                usize::from(phase != "before")
            );
        }
    }
}

#[test]
fn bl_update_failures_preserve_certainty_never_retry_or_refresh_and_cannot_edit_the_result() {
    for (response, expected) in [
        (
            json!({"status":403,"body":"{\"errors\":[{\"code\":5,\"message\":\"Denied test-secret\"}]}"}),
            "NotUpdated:",
        ),
        (json!({"status":500,"body":"{}"}), "Unknown:"),
        (
            json!({"body":"not json"}),
            "Updated; response processing failed",
        ),
        (
            json!({"body":"{\"id\":1}","truncate":true}),
            "Updated; response processing failed",
        ),
        (json!({"body":"{}"}), "Updated; response processing failed"),
    ] {
        let mut routes = update_routes();
        routes["routes"]["PATCH /api/v2/issues/B-1"] = response;
        let h = Harness::new(routes);
        settings(&h, "B");
        let result = scripted(
            &h,
            json!([
                {"wait":"TITLE: Detail B-1","keys":"Le"},
                {"wait":"Latest title","keys":"!\t\u{2}\t\t\t\r"},
                {"wait":"Confirm update","keys":"s"},
                {"wait":expected,"keys":"s\u{14}bad"},
                {"wait":expected,"keys":"\r"},
                {"wait":"> Issue Detail","keys":"q"}
            ]),
        );
        if expected != "NotUpdated:" {
            assert!(
                result["snapshots"][4]
                    .as_str()
                    .unwrap()
                    .contains("Do not resend"),
                "{result}"
            );
        }
        let requests = h.requests();
        assert_eq!(
            requests.iter().filter(|r| r["method"] == "PATCH").count(),
            1,
            "{requests:?}"
        );
        let patch = requests
            .iter()
            .position(|r| r["method"] == "PATCH")
            .unwrap();
        assert_eq!(patch, requests.len() - 1, "no follow-up GET: {requests:?}");
        assert!(
            !result["stdout"].as_str().unwrap().contains("test-secret"),
            "{result}"
        );
    }
}

#[test]
fn bl_update_cancelled_loading_and_older_detail_responses_cannot_replace_a_new_form_or_success() {
    let mut routes = update_routes();
    let mut latest = update_issue();
    latest["summary"] = json!("New generation title");
    latest["projectId"] = json!(3);
    let delayed = json!({"header_delay":1.4,"body":update_issue().to_string()});
    routes["routes"]["GET /api/v2/issues/B-1"]["sequence"] = json!([
        {"body":update_issue().to_string()}, delayed.clone(), delayed,
        {"body":latest.to_string()}
    ]);
    for suffix in ["", "/issueTypes", "/users", "/versions", "/statuses"] {
        routes["routes"][format!("/api/v2/projects/3{suffix}")] =
            routes["routes"][format!("/api/v2/projects/2{suffix}")].clone();
    }
    let h = Harness::new(routes);
    settings(&h, "B");
    let result = scripted(
        &h,
        json!([
            {"wait":"TITLE: Detail B-1","keys":"LR"},
            {"wait":"Loading... Esc: cancel","keys":"e", "pause":0.2,
             "wait_request":{"method":"GET","path":"/api/v2/issues/B-1","ordinal":2}},
            {"wait":"Loading issue / candidates","keys":"\u{1b}","pause":0.2,
             "wait_request":{"method":"GET","path":"/api/v2/issues/B-1","ordinal":3}},
            {"wait":"> Issue Detail","keys":"e"},
            {"wait":"New generation title","keys":"!\t\u{2}\t\t\t\r", "pause":1.6},
            {"wait":"Confirm update","keys":"s"},
            {"wait":"Comment display refreshed","keys":"\r"},
            {"wait":"Changed title","keys":"q", "pause":0.3}
        ]),
    );
    let requests = h.requests();
    let patches: Vec<_> = requests.iter().filter(|r| r["method"] == "PATCH").collect();
    assert_eq!(patches.len(), 1, "{requests:?}");
    assert_eq!(patches[0]["path"], "/api/v2/issues/B-1", "{requests:?}");
    assert!(
        result["snapshots"][4]
            .as_str()
            .unwrap()
            .contains("New generation title"),
        "{result}"
    );
    assert!(
        !result["snapshots"][4].as_str().unwrap().contains("ERROR"),
        "{result}"
    );
    for suffix in ["", "/issueTypes", "/users", "/versions", "/statuses"] {
        let path = format!("/api/v2/projects/3{suffix}");
        assert!(
            requests
                .iter()
                .any(|r| r["method"] == "GET" && r["path"] == path),
            "latest project must drive rules and candidates: {requests:?}"
        );
    }
    for suffix in ["/issueTypes", "/users", "/versions", "/statuses"] {
        let path = format!("/api/v2/projects/2{suffix}");
        assert!(
            !requests
                .iter()
                .any(|r| r["method"] == "GET" && r["path"] == path),
            "old project candidates must not be used: {requests:?}"
        );
    }
    assert_eq!(
        requests
            .iter()
            .filter(|r| r["method"] == "GET" && r["path"] == "/api/v2/issues/B-1")
            .count(),
        4,
        "{requests:?}"
    );
}

#[test]
fn bl_update_validation_retains_draft_and_tiny_terminal_cannot_edit_confirm_or_send() {
    let h = Harness::new(update_routes());
    settings(&h, "B");
    let result = scripted(
        &h,
        json!([
            {"wait":"TITLE: Detail B-1","keys":"Le"},
            {"wait":"Latest title","keys":"\u{15}\t\u{2}\t\t\t\r"},
            {"wait":"Title must not be blank","keys":"\u{14}Draft qhljkHL\t\t\t\t2023-02-29\t\t\u{2}\t\t\t\r"},
            {"wait":"Date must be a real","keys":"\u{14}\t\t\t\t\u{15}2024-02-29\t\t\u{2}\t\t\t"},
            {"wait":"Review /", "resize":[11,39]},
            {"wait":"Terminal too small","keys":"s\r\u{14}destroy"},
            {"wait":"Terminal too small","resize":[12,40]},
            {"wait":"Review /", "keys":"\r"},
            {"wait":"Confirm update","keys":"\r","resize":[11,39]},
            {"wait":"Terminal too small","keys":"s\r"},
            {"wait":"Terminal too small","resize":[24,100]},
            {"wait":"Draft qhljkHL","keys":"\u{1b}"},
            {"wait":"Review /","keys":"\u{1b}"},
            {"wait":"Discard changes?","keys":"\u{1b}"},
            {"wait":"Draft qhljkHL","keys":"\u{1b}"},
            {"wait":"Discard changes?","keys":"\r"},
            {"wait":"> Issue Detail","keys":"q"}
        ]),
    );
    assert!(h.requests().iter().all(|r| r["method"] == "GET"));
    assert!(
        !result["stdout"].as_str().unwrap().contains("destroy"),
        "{result}"
    );
}

#[test]
fn bl_update_safe_display_keeps_raw_title_and_zero_width_candidate_without_reconstructing_secrets()
{
    let mut routes = update_routes();
    routes["routes"]["/api/v2/projects/2/issueTypes"]["body"] =
        json!(json!([{"id":2,"name":"test-\u{200b}secret"}]).to_string());
    let h = Harness::new(routes);
    settings(&h, "B");
    let raw = " Raw qhljkHL test-secret\u{1b}]52;c;clipboard\u{7} 日本語 e\u{301} 👩‍💻 ";
    let result = scripted(
        &h,
        json!([
            {"wait":"TITLE: Detail B-1","keys":"Le"},
            {"wait":"Latest title","keys":format!("\u{15}\u{1b}[200~{raw}\u{1b}[201~\t\r")},
            {"wait":"Issue type candidates","keys":"\r","resize":[12,40]},
            {"wait":"Issue type:","keys":"\u{14}"},
            {"wait":"[REDACTED]","keys":"\t\u{2}\t\t\t\r", "resize":[24,100]},
            {"wait":"Confirm update","keys":"s"},
            {"wait":"Comment display refreshed","keys":"\r"},
            {"wait":"> Issue Detail","keys":"q"}
        ]),
    );
    let output = result["stdout"].as_str().unwrap();
    assert!(
        !output.contains("test-secret") && !output.contains("\u{1b}]52;c;clipboard"),
        "{result}"
    );
    assert!(output.contains("test-\\u{200b}secret"), "{result}");
    let patches: Vec<_> = h
        .requests()
        .into_iter()
        .filter(|r| r["method"] == "PATCH")
        .collect();
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0]["form"], json!({"summary":[raw]}));
}

#[test]
fn bl_update_editor_failure_keeps_latest_raw_body_other_fields_and_focus() {
    let h = Harness::new(update_routes());
    settings(&h, "B");
    h.config(json!({"editor":["/usr/bin/python3",format!("{}/tests/fixtures/editor.py",env!("CARGO_MANIFEST_DIR")),h.root.join("editor-record").display().to_string(),"nonzero"],"profiles":{"default":{"spaceUrl":h.origin,"apiKeyEnv":"TEST_KEY"}}}));
    let result = scripted(
        &h,
        json!([
            {"wait":"TITLE: Detail B-1","keys":"Le"},
            {"wait":"Latest title","keys":"!\t\u{2}\r"},
            {"wait":"EDITOR READY","keys":"\n","resize":[12,40]},
            {"wait":"Editor did not exit successfully","keys":"\t\t\t\r","resize":[24,100]},
            {"wait":"Confirm update","keys":"s"},
            {"wait":"Comment display refreshed","keys":"\r"},
            {"wait":"> Issue Detail","keys":"q"}
        ]),
    );
    let record: Value =
        serde_json::from_slice(&std::fs::read(h.root.join("editor-record")).unwrap()).unwrap();
    assert_eq!(record["original"], "Original body");
    assert_eq!(record["mode"], json!(0o600));
    assert_eq!(record["cooked"], true);
    assert_eq!(record["echo"], true);
    assert!(!std::path::Path::new(record["path"].as_str().unwrap()).exists());
    let patches: Vec<_> = h
        .requests()
        .into_iter()
        .filter(|r| r["method"] == "PATCH")
        .collect();
    assert_eq!(patches.len(), 1, "{result}");
    assert_eq!(patches[0]["form"], json!({"summary":["Latest title!"]}));
}

#[test]
fn bl_update_local_acceptance_fixture_refreshes_comments_but_preserves_old_list_with_one_patch() {
    let h = Harness::new(serde_json::from_str(include_str!("fixtures/tui_update.json")).unwrap());
    settings(&h, "B");
    let result = scripted(
        &h,
        serde_json::from_str(include_str!("fixtures/tui_update_steps.json")).unwrap(),
    );
    assert_eq!(
        result["request_counts"][4], result["request_counts"][5],
        "{result}"
    );
    let screen = result["snapshots"][8].as_str().unwrap();
    for text in [
        "Changed title",
        "Old list title",
        "TITLE: Latest title -> Changed title",
    ] {
        assert!(screen.contains(text), "{result}");
    }
    let requests = h.requests();
    assert_eq!(requests.len(), 19, "{requests:?}");
    let patches: Vec<_> = requests.iter().filter(|r| r["method"] == "PATCH").collect();
    assert_eq!(patches.len(), 1);
    assert_eq!(
        patches[0]["form"],
        json!({"summary":["Changed title"],"description":[""],"assigneeId":[""],"resolutionId":["0"],"milestoneId[]":["5","6"]})
    );
}

#[test]
fn bl_update_preserves_list_page_history_selection_scroll_but_resets_comments_to_latest() {
    let mut routes = update_routes();
    routes["routes"]["/api/v2/issues/count"] = json!({"body":"{\"count\":40}"});
    routes["routes"]["GET /api/v2/issues"] =
        json!({"offsets":{"0":{"body":issue_items(1,20)},"20":{"body":issue_items(21,40)}}});
    let mut latest = update_issue();
    latest["issueKey"] = json!("B-38");
    latest["summary"] = json!("Page two title");
    routes["routes"]["GET /api/v2/issues/B-38"] = json!({"body":latest.to_string()});
    latest["summary"] = json!("Updated page two title");
    routes["routes"]["PATCH /api/v2/issues/B-38"] = json!({"body":latest.to_string()});
    let comment: Value = serde_json::from_str(include_str!("fixtures/tui_comment.json")).unwrap();
    routes["routes"]["/api/v2/issues/B-38/comments"] =
        comment["routes"]["/api/v2/issues/B-1/comments"]["sequence"][0].clone();
    let h = Harness::new(routes);
    settings(&h, "B");
    let result = scripted(
        &h,
        json!([
            {"wait":"TITLE: Detail B-1","keys":"ln"},
            {"wait":"> B-21","keys":"jjjjjjjjjjjjjjjjj"},
            {"wait":"TITLE: Page two title","keys":"Ll"},
            {"wait":"Comment 2 line 01","keys":"n"},
            {"wait":"Comment 1 line 01","keys":"jjjjjjjjjjjjjjj"},
            {"wait":"Comment 1 line 16","keys":"he"},
            {"wait":"Page two title","keys":"\u{15}Updated page two title\t\u{2}\t\t\t\r"},
            {"wait":"Confirm update","keys":"s"},
            {"wait":"Comment display refreshed","keys":"\r"},
            {"wait":"Updated page two title","keys":"l"},
            {"wait":"Comment 2 line 01","keys":"H"},
            {"wait":"> B-38","keys":"p"},
            {"wait":"> B-1","keys":"n"},
            {"wait":"> B-38","keys":"q"}
        ]),
    );
    let snapshots = result["snapshots"].as_array().unwrap();
    let list = |i: usize| {
        snapshots[i]
            .as_str()
            .unwrap()
            .lines()
            .skip(11)
            .take(11)
            .map(|line| line.chars().skip(1).take(38).collect::<String>())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        list(5),
        list(10),
        "update changed list selection/scroll: {result}"
    );
    assert_eq!(list(10), list(13), "update lost visited page: {result}");
    assert!(
        snapshots[10]
            .as_str()
            .unwrap()
            .contains("Comment 2 line 01"),
        "{result}"
    );
    assert!(
        !snapshots[10]
            .as_str()
            .unwrap()
            .contains("Comment 1 line 16"),
        "{result}"
    );
    assert_eq!(
        result["request_counts"][9], result["request_counts"][11],
        "focus/close caused extra GET: {result}"
    );
    let requests = h.requests();
    assert_eq!(
        requests
            .iter()
            .filter(|r| r["path"]
                .as_str()
                .unwrap()
                .starts_with("/api/v2/issues/count?"))
            .count(),
        1
    );
    assert_eq!(
        requests
            .iter()
            .filter(|r| r["path"].as_str().unwrap().starts_with("/api/v2/issues?"))
            .count(),
        2
    );
    assert_eq!(
        requests.iter().filter(|r| r["method"] == "PATCH").count(),
        1
    );
}

#[test]
fn bl_update_invalidates_pending_list_and_comment_requests_and_keeps_the_fixed_detail_target() {
    for failing in [false, true] {
        for operation in ["page", "list-refresh", "comment-oldest", "comment-refresh"] {
            let mut routes = update_routes();
            let comment = |content: &str| {
                json!([{"id":1,"content":content,"changeLog":[],"createdUser":{"id":7,"name":"Author"},"created":"2026-03-01T00:00:00Z","updated":"2026-03-01T00:00:00Z"}]).to_string()
            };
            routes["routes"]["/api/v2/issues/B-1/comments"] = json!({"sequence":[
                {"body":comment("Retained comment")},
                if failing {json!({"header_delay":1.2,"status":403})} else {json!({"header_delay":1.2,"body":comment("Late comment must not replace selection")})},
                {"header_delay":1.4,"body":comment("Fresh automatic comment")}
            ]});
            routes["routes"]["/api/v2/issues/count"] = json!({"body":"{\"count\":40}"});
            routes["routes"]["GET /api/v2/issues"] = if operation == "list-refresh" {
                json!({"sequence":[{"body":issue_items(1,20)}, if failing {json!({"header_delay":1.2,"status":403})} else {json!({"header_delay":1.2,"body":issue_items(21,20)})}]})
            } else {
                json!({"offsets":{"0":{"body":issue_items(1,20)},"20":if failing {json!({"header_delay":1.2,"status":403})} else {json!({"header_delay":1.2,"body":issue_items(21,20)})}}})
            };
            if !operation.starts_with("comment") {
                routes["routes"]["/api/v2/issues/B-1/comments"]["sequence"]
                    .as_array_mut()
                    .unwrap()
                    .remove(1);
            }
            let h = Harness::new(routes);
            settings(&h, "B");
            let result = scripted(
                &h,
                json!([
                    {"wait":"Retained comment","keys":match operation {
                        "page" => "ln", "list-refresh" => "lR", "comment-oldest" => "LlG", _ => "LlR"
                    }},
                    {"wait":"Loading... Esc: cancel","pause":0.2,"keys":if operation.starts_with("comment") {"he"} else {"Le"}},
                    {"wait":"Latest title","pause":0.1,"keys":"!\t\u{2}\t\t\t\r"},
                    {"wait":"Confirm update","keys":"s"},
                    {"wait":"Comment display refreshed","keys":"\r"},
                    {"wait":"Changed title","keys":"H"},
                    {"wait":"> B-1","keys":"p"},
                    {"wait":"> B-1","keys":"LR"},
                    {"wait":"TITLE: Latest title","keys":"q"}
                ]),
            );
            let retained = result["snapshots"][6].as_str().unwrap();
            assert!(
                !retained.contains("Failed:"),
                "stale failure replaced current notice: {result}"
            );
            for text in ["> B-1", "Changed title", "Fresh automatic comment"] {
                assert!(
                    retained.contains(text),
                    "{operation}: missing {text}: {result}"
                );
            }
            assert_eq!(
                result["request_counts"][6], result["request_counts"][7],
                "history changed: {result}"
            );
            let requests = h.requests();
            assert_eq!(
                requests.iter().filter(|r| r["method"] == "PATCH").count(),
                1,
                "{requests:?}"
            );
            assert_eq!(
                requests
                    .iter()
                    .filter(|r| r["method"] == "GET" && r["path"] == "/api/v2/issues/B-1")
                    .count(),
                5,
                "{requests:?}"
            );
            assert!(
                !requests.iter().any(|r| r["path"] == "/api/v2/issues/B-21"),
                "{requests:?}"
            );
        }
    }
}

#[test]
fn bl_update_incomplete_or_mistyped_success_preserves_detail_and_reports_updated_without_retry() {
    let mut mistyped_summary = update_issue();
    mistyped_summary["summary"] = json!(false);
    let mut invalid_milestone = update_issue();
    invalid_milestone["milestone"] = json!([{"id":5,"name":null}]);
    for response in [
        json!({"id":1,"issueKey":"B-1"}),
        mistyped_summary,
        invalid_milestone,
    ] {
        let mut routes = update_routes();
        routes["routes"]["PATCH /api/v2/issues/B-1"] = json!({"body":response.to_string()});
        let h = Harness::new(routes);
        settings(&h, "B");
        let result = scripted(
            &h,
            json!([
                {"wait":"TITLE: Detail B-1","keys":"Le"},
                {"wait":"Latest title","keys":"!\t\u{2}\t\t\t\r"},
                {"wait":"Confirm update","keys":"s"},
                {"wait":"Updated; response processing failed","keys":"s\r"},
                {"wait":"TITLE: Detail B-1","keys":"q"}
            ]),
        );
        assert!(
            result["snapshots"][3]
                .as_str()
                .unwrap()
                .contains("Do not resend"),
            "{response}: {result}"
        );
        let requests = h.requests();
        assert_eq!(
            requests.iter().filter(|r| r["method"] == "PATCH").count(),
            1,
            "{requests:?}"
        );
        assert_eq!(
            requests.last().unwrap()["method"],
            "PATCH",
            "no follow-up GET: {requests:?}"
        );
    }
}

#[test]
fn bl_update_latest_get_for_another_key_fails_before_candidates_or_form_initialization() {
    let mut routes = update_routes();
    let mut another = update_issue();
    another["issueKey"] = json!("B-2");
    another["summary"] = json!("Other issue must not initialize form");
    routes["routes"]["GET /api/v2/issues/B-1"]["sequence"][1]["body"] = json!(another.to_string());
    let h = Harness::new(routes);
    settings(&h, "B");
    let result = scripted(
        &h,
        json!([
            {"wait":"TITLE: Detail B-1","keys":"Le"},
            {"wait":"Issue response does not match selected key","keys":"s"},
            {"wait":"Issue response does not match selected key","keys":"\u{1b}"},
            {"wait":"TITLE: Detail B-1","keys":"q"}
        ]),
    );
    assert!(
        !result["stdout"]
            .as_str()
            .unwrap()
            .contains("Other issue must not initialize form"),
        "{result}"
    );
    let requests = h.requests();
    assert_eq!(
        requests.last().unwrap()["path"],
        "/api/v2/issues/B-1",
        "no candidate GET after mismatched issue: {requests:?}"
    );
    assert!(requests.iter().all(|r| r["method"] == "GET"));
    assert_eq!(
        requests
            .iter()
            .filter(|r| r["path"] == "/api/v2/issues/B-1")
            .count(),
        2,
        "{requests:?}"
    );
}

#[test]
fn bl_update_patch_success_for_another_key_keeps_old_detail_and_updated_certainty() {
    let mut routes = update_routes();
    let mut another = update_issue();
    another["issueKey"] = json!("B-2");
    another["summary"] = json!("Other issue must not replace detail");
    routes["routes"]["PATCH /api/v2/issues/B-1"] = json!({"body":another.to_string()});
    let h = Harness::new(routes);
    settings(&h, "B");
    let result = scripted(
        &h,
        json!([
            {"wait":"TITLE: Detail B-1","keys":"Le"},
            {"wait":"Latest title","keys":"!\t\u{2}\t\t\t\r"},
            {"wait":"Confirm update","keys":"s"},
            {"wait":"Updated; response processing failed","keys":"s\r"},
            {"wait":"TITLE: Detail B-1","keys":"q"}
        ]),
    );
    assert!(
        result["snapshots"][3]
            .as_str()
            .unwrap()
            .contains("Do not resend"),
        "{result}"
    );
    assert!(
        !result["stdout"]
            .as_str()
            .unwrap()
            .contains("Other issue must not replace detail"),
        "{result}"
    );
    let requests = h.requests();
    assert_eq!(
        requests.iter().filter(|r| r["method"] == "PATCH").count(),
        1,
        "{requests:?}"
    );
    assert_eq!(
        requests.last().unwrap()["method"],
        "PATCH",
        "no follow-up GET: {requests:?}"
    );
}

#[test]
fn bl_update_comment_refresh_wait_blocks_closing_editing_focus_and_tiny_quit_but_allows_scroll_resize()
 {
    let key = "B-123456789012345678901234567890123456789012345678901234567890";
    let mut routes: Value =
        serde_json::from_str(&include_str!("fixtures/tui_update.json").replace("B-1", key))
            .unwrap();
    routes["routes"][format!("/api/v2/issues/{key}/comments")]["sequence"][1]["header_delay"] =
        json!(2.5);
    let h = Harness::new(routes);
    settings(&h, "B");
    let result = scripted(
        &h,
        json!([
            {"wait":"Retained comment", "keys":"Le"},
            {"wait":"Latest title", "keys":"!\t\u{2}\t\t\t\r"},
            {"wait":"Confirm update", "keys":"s"},
            {"wait":"Updated; refreshing comments", "keys":"\r\u{1b}seHLlnpR", "resize":[12,40]},
            {"wait":"Refreshing: please wait", "keys":"G"},
            {"wait":"Refreshing: please wait", "keys":"g"},
            {"wait":"Updated: B-1", "keys":"\r\u{1b}sHeL", "resize":[11,39]},
            {"wait":"Terminal too small", "keys":"q\r\u{1b}seHL", "resize":[24,100]},
            {"wait":"Updated; refreshing comments", "keys":"G"},
            {"wait":"Refreshing: please wait", "keys":"g"},
            {"wait":"Updated; refreshing comments"},
            {"wait":"Comment display refreshed", "keys":"\u{1b}"},
            {"wait":"> Issue Detail", "keys":"q"}
        ]),
    );
    let snapshots = result["snapshots"].as_array().unwrap();
    assert!(
        !snapshots[3].as_str().unwrap().contains("Esc: cancel"),
        "{result}"
    );
    for i in [3, 8, 10] {
        assert!(
            snapshots[i]
                .as_str()
                .unwrap()
                .contains("Updated; refreshing comments"),
            "{result}"
        );
    }
    assert!(
        !snapshots[5].as_str().unwrap().contains("Updated: B-1"),
        "result did not scroll at 40x12: {result}"
    );
    assert!(
        snapshots[6].as_str().unwrap().contains("Updated: B-1"),
        "result did not scroll home: {result}"
    );
    assert!(
        snapshots[12]
            .as_str()
            .unwrap()
            .contains("TITLE: Latest title -> Changed title"),
        "{result}"
    );
    let requests = h.requests();
    let patch = requests
        .iter()
        .position(|r| r["method"] == "PATCH")
        .unwrap();
    assert_eq!(
        requests.len() - patch,
        2,
        "one PATCH then only history Latest: {requests:?}"
    );
    assert_eq!(
        requests[patch + 1]["path"],
        format!("/api/v2/issues/{key}/comments?count=20&order=desc")
    );
}

#[test]
fn bl_update_comment_refresh_body_and_empty_success_replace_only_after_preparation() {
    for empty in [false, true] {
        let mut routes: Value =
            serde_json::from_str(include_str!("fixtures/tui_update.json")).unwrap();
        let mut item: Value = serde_json::from_str(
            routes["routes"]["/api/v2/issues/B-1/comments"]["sequence"][0]["body"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        item[0]["id"] = json!(97);
        item[0]["content"] = json!("Other person's newest comment\nSecond body line");
        routes["routes"]["/api/v2/issues/B-1/comments"]["sequence"][1] =
            json!({"body":if empty {"[]".into()} else {item.to_string()}});
        let h = Harness::new(routes);
        settings(&h, "B");
        let result = scripted(
            &h,
            json!([
                {"wait":"Retained comment", "keys":"Le"},
                {"wait":"Latest title", "keys":"!\t\u{2}\t\t\t\r"},
                {"wait":"Confirm update", "keys":"s"},
                {"wait":"Comment display refreshed", "keys":"\r"},
                {"wait":if empty {"No comments"} else {"Other person's newest comment"}, "keys":"q"}
            ]),
        );
        let requests = h.requests();
        let patch = requests
            .iter()
            .position(|r| r["method"] == "PATCH")
            .unwrap();
        let after = &requests[patch + 1..];
        assert_eq!(after.len(), if empty { 1 } else { 3 }, "{requests:?}");
        assert_eq!(
            after[0]["path"],
            "/api/v2/issues/B-1/comments?count=20&order=desc"
        );
        if !empty {
            assert_eq!(after[1]["path"], "/api/v2/issues/B-1");
            assert_eq!(after[2]["path"], "/api/v2/projects/2");
        }
        let final_screen = result["snapshots"][4].as_str().unwrap();
        assert!(!final_screen.contains("Retained comment"), "{result}");
        assert!(final_screen.contains("TITLE: Changed title"), "{result}");
    }
}

#[test]
fn bl_update_comment_refresh_failures_keep_selected_id_scroll_and_updated_detail_until_manual_r_succeeds()
 {
    for failure in ["reject", "disconnect", "json", "invalid", "rule", "preview"] {
        let mut routes = update_routes();
        let comments: Value =
            serde_json::from_str(include_str!("fixtures/tui_comment.json")).unwrap();
        let old = comments["routes"]["/api/v2/issues/B-1/comments"]["sequence"][0].clone();
        let mut page = old["commentItems"].clone();
        page[0]["content"] = json!("Fresh manual recovery");
        let success = json!({"commentItems":page.clone()});
        let failed = match failure {
            "reject" => {
                json!({"status":403,"body":"{\"errors\":[{\"code\":5,\"message\":\"Denied test-secret\"}]}"})
            }
            "disconnect" => json!({"disconnect":true}),
            "json" => json!({"body":"not json"}),
            "invalid" => json!({"body":"[{\"id\":9}]"}),
            "preview" => {
                page[1]["changeLog"] = json!([{"field":"custom","originalValue":{"test-secret":1,"[REDACTED]":2},"newValue":null}]);
                json!({"body":page.to_string()})
            }
            _ => {
                // Initial Detail + initial comment + form, then automatic rule fails.
                routes["routes"]["/api/v2/projects/2"] = json!({"sequence":[
                    {"body":"{\"textFormattingRule\":\"markdown\"}"},
                    {"body":"{\"textFormattingRule\":\"markdown\"}"},
                    {"body":"{\"textFormattingRule\":\"markdown\"}"},
                    {"status":403},
                    {"body":"{\"textFormattingRule\":\"markdown\"}"}
                ]});
                success.clone()
            }
        };
        routes["routes"]["/api/v2/issues/B-1/comments"] =
            json!({"sequence":[old, failed, {"status":503}, success]});
        let h = Harness::new(routes);
        settings(&h, "B");
        let result = scripted(
            &h,
            json!([
                {"wait":"Comment 2 line 01", "keys":"Lln"},
                {"wait":"Comment 1 line 01", "keys":"jjjjjjjjjjjjjjj"},
                {"wait":"Comment 1 line 16", "keys":"he"},
                {"wait":"Latest title", "keys":"!\t\u{2}\t\t\t\r"},
                {"wait":"Confirm update", "keys":"s"},
                {"wait":"Comment display refresh failed", "keys":"\r"},
                {"wait":"> Issue Detail", "keys":"lR"},
                {"wait":"Failed:", "keys":"\r"},
                {"wait":"> Comment", "keys":"R"},
                {"wait":"Comment 2 line 01", "keys":"n"},
                {"wait":"Fresh manual recovery", "keys":"q"}
            ]),
        );
        let snapshots = result["snapshots"].as_array().unwrap();
        for i in [6, 7, 8] {
            let screen = snapshots[i].as_str().unwrap();
            assert!(screen.contains("Comment 1 line 16"), "{failure}: {result}");
            assert!(
                screen.contains("TITLE: Changed title"),
                "{failure}: {result}"
            );
        }
        let notice = snapshots[5].as_str().unwrap();
        for text in [
            "Updated",
            "Old display retained",
            "Do not resend",
            "Comment R",
        ] {
            assert!(notice.contains(text), "{failure}: missing {text}: {result}");
        }
        if matches!(failure, "reject" | "rule") {
            assert!(notice.contains("HTTP 403"), "HTTP status missing: {result}");
        }
        assert!(
            !result["stdout"].as_str().unwrap().contains("test-secret"),
            "{result}"
        );
        let requests = h.requests();
        assert_eq!(
            requests.iter().filter(|r| r["method"] == "PATCH").count(),
            1,
            "{requests:?}"
        );
        assert_eq!(
            requests
                .iter()
                .filter(|r| r["path"] == "/api/v2/issues/B-1/comments?count=20&order=desc")
                .count(),
            4,
            "{requests:?}"
        );
        for prefix in ["/api/v2/issues/count?", "/api/v2/issues?"] {
            assert_eq!(
                requests
                    .iter()
                    .filter(|r| r["path"].as_str().unwrap().starts_with(prefix))
                    .count(),
                1,
                "{requests:?}"
            );
        }
    }
}

#[test]
fn bl_update_comment_refresh_interrupt_preserves_updated_certainty_and_restores_terminal() {
    for signal in [false, true] {
        for tiny in [false, true] {
            let mut routes: Value =
                serde_json::from_str(include_str!("fixtures/tui_update.json")).unwrap();
            routes["routes"]["/api/v2/issues/B-1/comments"]["sequence"][1]["header_delay"] =
                json!(20);
            let h = Harness::new(routes);
            settings(&h, "B");
            let mut last = json!({"wait":if tiny {"Terminal too small"} else {"Updated; refreshing comments"}});
            if signal {
                last["signal"] = json!("SIGINT");
            } else {
                last["keys"] = json!("\u{3}");
            }
            std::fs::write(h.root.join("shell-steps.json"), json!([
                {"wait":"Retained comment", "keys":"Le"},
                {"wait":"Latest title", "keys":"!\t\u{2}\t\t\t\r"},
                {"wait":"Confirm update", "keys":"s"},
                {"wait":"Updated; refreshing comments", "resize":if tiny {json!([11,39])} else {json!([24,100])}},
                last
            ]).to_string()).unwrap();
            let result = shell(&h, "scripted", &[]);
            assert_eq!(result["code"], 130, "{result}");
            assert_eq!(result["restored"], true, "{result}");
            assert_eq!(result["termios_diff"], json!([]), "{result}");
            assert!(result["elapsed"].as_f64().unwrap() < 4.0, "{result}");
            let stderr = result["stderr"].as_str().unwrap();
            assert!(
                stderr.contains("; updated:") && stderr.contains("do not resend"),
                "{result}"
            );
            assert!(
                !result["stdout"]
                    .as_str()
                    .unwrap()
                    .contains("Comment display refreshed"),
                "{result}"
            );
            let requests = h.requests();
            assert_eq!(
                requests.iter().filter(|r| r["method"] == "PATCH").count(),
                1,
                "{requests:?}"
            );
            assert_eq!(
                requests.last().unwrap()["path"],
                "/api/v2/issues/B-1/comments?count=20&order=desc"
            );
        }
    }
}

#[test]
fn bl_update_comment_refresh_failure_retains_empty_or_unfetched_state_instead_of_successful_empty()
{
    for empty in [false, true] {
        let mut routes = update_routes();
        routes["routes"]["/api/v2/issues/B-1/comments"] = json!({"sequence":[
            if empty {json!({"body":"[]"})} else {json!({"header_delay":20,"body":"[]"})},
            {"status":403}, {"body":"[]"}
        ]});
        let h = Harness::new(routes);
        settings(&h, "B");
        let result = scripted(
            &h,
            json!([
                {"wait":if empty {"No comments"} else {"TITLE: Detail B-1"}, "keys":"Le"},
                {"wait":"Latest title", "keys":"!\t\u{2}\t\t\t\r"},
                {"wait":"Confirm update", "keys":"s"},
                {"wait":"Comment display refresh failed", "keys":"\u{1b}"},
                {"wait":"> Issue Detail", "keys":"lR"},
                {"wait":"No comments", "keys":"q"}
            ]),
        );
        let retained = result["snapshots"][4].as_str().unwrap();
        assert_eq!(retained.contains("No comments"), empty, "{result}");
        if !empty {
            assert!(
                retained.lines().nth(21).unwrap().contains("B-1"),
                "{result}"
            );
        }
        assert!(retained.contains("TITLE: Changed title"), "{result}");
        let requests = h.requests();
        assert_eq!(
            requests
                .iter()
                .filter(|r| r["path"] == "/api/v2/issues/B-1/comments?count=20&order=desc")
                .count(),
            3,
            "{requests:?}"
        );
        let patch = requests
            .iter()
            .position(|r| r["method"] == "PATCH")
            .unwrap();
        assert_eq!(
            requests.len() - patch,
            3,
            "one PATCH plus automatic and manual Latest only: {requests:?}"
        );
    }
}

#[test]
fn bl_comment_add_body_only_confirms_then_posts_once_and_refreshes_without_touching_list() {
    let mut routes = update_routes();
    routes["routes"]["POST /api/v2/issues/B-1/comments"] =
        json!({"status":201,"body":"{\"id\":99}"});
    let h = Harness::new(routes);
    settings(&h, "B");
    h.config(json!({"editor":["/usr/bin/python3",format!("{}/tests/fixtures/editor.py",env!("CARGO_MANIFEST_DIR")),h.root.join("editor-record"),"body-file"],"profiles":{"default":{"spaceUrl":h.origin,"apiKeyEnv":"TEST_KEY"}}}));
    let body = " Raw comment 日本語\n test-secret\u{1b}]52;c;unsafe\u{7}\n";
    std::fs::write(h.root.join("editor-body"), body).unwrap();
    let result = scripted(
        &h,
        json!([
            {"wait":"TITLE: Detail B-1","keys":"Llc"},
            {"wait":"Add comment","keys":"\r"},
            {"wait":"EDITOR READY","keys":"\n"},
            {"wait":"Body editor finished","keys":"\t\r\r\t\t\t\r"},
            {"wait":"Confirm comment","keys":"\r"},
            {"wait":"Confirm comment","keys":"s"},
            {"wait":"Comment display refreshed","keys":"\r"},
            {"wait":"> Comment","keys":"q"}
        ]),
    );
    let counts = result["request_counts"].as_array().unwrap();
    assert_eq!(counts[4], counts[5], "Enter sent: {result}");
    let requests = h.requests();
    let writes: Vec<_> = requests.iter().filter(|r| r["method"] != "GET").collect();
    assert_eq!(writes.len(), 1, "{requests:?}");
    assert_eq!(writes[0]["method"], "POST");
    assert_eq!(writes[0]["path"], "/api/v2/issues/B-1/comments");
    assert_eq!(writes[0]["form"], json!({"content":[body]}));
    let sent = requests.iter().position(|r| r["method"] == "POST").unwrap();
    assert!(
        requests[sent + 1..]
            .iter()
            .all(|r| r["path"] != "/api/v2/issues" && r["path"] != "/api/v2/issues/count")
    );
    assert!(!result["stdout"].as_str().unwrap().contains("test-secret"));
    assert!(!result["stdout"].as_str().unwrap().contains("\u{1b}]52;"));
}

#[test]
fn bl_comment_add_patch_confirmation_interrupt_keeps_comment_operation_and_not_updated() {
    let h = Harness::new(update_routes());
    settings(&h, "B");
    std::fs::write(
        h.root.join("shell-steps.json"),
        json!([
            {"wait":"TITLE: Detail B-1","keys":"Llc"},
            {"wait":"Add comment","keys":"\t\rj\r\t\t\t\r"},
            {"wait":"Confirm comment","keys":"\u{3}"}
        ])
        .to_string(),
    )
    .unwrap();
    let result = shell(&h, "scripted", &[]);
    assert_eq!(result["code"], 130, "{result}");
    assert_eq!(result["restored"], true);
    let stderr = result["stderr"].as_str().unwrap();
    assert!(stderr.contains("not_updated: comment.add"), "{result}");
    assert!(h.requests().iter().all(|r| r["method"] == "GET"));
}

fn comment_add_routes() -> Value {
    let mut routes = update_routes();
    let mut latest = update_issue();
    latest["assignee"] = json!({"id":4,"name":"Latest owner"});
    latest["resolution"] = json!({"id":8,"name":"Latest resolution"});
    routes["routes"]["GET /api/v2/issues/B-1"]["sequence"][1]["body"] = json!(latest.to_string());
    routes["routes"]["POST /api/v2/issues/B-1/comments"] =
        json!({"status":201,"body":"{\"id\":99}"});
    routes["routes"]["/api/v2/issues/B-1/comments"] = json!({"sequence":[
        {"body":json!([comment_item(1,json!(null),json!([{"field":"old","originalValue":null,"newValue":"OLD COMMENT"}]))]).to_string()},
        {"header_delay":0.3,"body":json!([comment_item(99,json!(null),json!([{"field":"new","originalValue":null,"newValue":"LATEST REGISTERED"}]))]).to_string()}
    ]});
    routes
}

#[test]
fn bl_comment_add_latest_fields_no_defaults_independent_changes_clears_and_resolution_zero() {
    for (keys, expected, body) in [
        ("\t\rj\r\t\t\t\r", json!({"statusId":["2"]}), false),
        (
            "\t\rj\r\t\t\t\r",
            json!({"statusId":["2"],"comment":[" Raw comment\n\n"]}),
            true,
        ),
        (
            "\t\t\u{1b}[3~\t\u{1b}[3~\t\r",
            json!({"assigneeId":[""],"resolutionId":[""]}),
            false,
        ),
        ("\t\t\t\rj\r\t\r", json!({"resolutionId":["0"]}), false),
    ] {
        let h = Harness::new(comment_add_routes());
        settings(&h, "B");
        std::fs::write(h.root.join(".backlog/settings.json"), json!({"project":"B","issue":{"create":{"issueType":"DO NOT USE","priority":"DO NOT USE","assignee":"DO NOT USE"}}}).to_string()).unwrap();
        let mut steps = vec![json!({"wait":"OLD COMMENT","keys":"Llc"})];
        if body {
            h.config(json!({"editor":["/usr/bin/python3",format!("{}/tests/fixtures/editor.py",env!("CARGO_MANIFEST_DIR")),h.root.join("editor-record"),"body-file"],"profiles":{"default":{"spaceUrl":h.origin,"apiKeyEnv":"TEST_KEY"}}}));
            std::fs::write(h.root.join("editor-body"), " Raw comment\n\n").unwrap();
            steps.extend([
                json!({"wait":"Add comment","keys":"\r"}),
                json!({"wait":"EDITOR READY","keys":"\n"}),
            ]);
        }
        steps.extend([
            json!({"wait":"Latest owner (4)","keys":keys}),
            json!({"wait":"Confirm comment","keys":"\r"}),
            json!({"wait":"Confirm comment","keys":"s"}),
            json!({"wait":"Sending","keys":"ss\r\u{1b}\u{2}"}),
            json!({"wait":"Comment display refreshed","keys":"\r"}),
            json!({"wait":"LATEST REGISTERED","keys":"l"}),
            json!({"wait":"Changed title","keys":"H"}),
            json!({"wait":"Selected project issue","keys":"q"}),
        ]);
        let result = scripted(&h, json!(steps));
        let requests = h.requests();
        let writes: Vec<_> = requests.iter().filter(|r| r["method"] != "GET").collect();
        assert_eq!(writes.len(), 1, "{requests:?}");
        assert_eq!(writes[0]["method"], "PATCH");
        assert_eq!(writes[0]["form"], expected, "{requests:?}");
        assert_eq!(
            requests
                .iter()
                .filter(|r| r["path"] == "/api/v2/issues/B-1" && r["method"] == "GET")
                .count(),
            2,
            "no confirmation GET: {requests:?}"
        );
        assert_eq!(
            requests
                .iter()
                .filter(|r| r["path"]
                    .as_str()
                    .unwrap()
                    .starts_with("/api/v2/issues/B-1/comments?"))
                .count(),
            2
        );
        assert!(
            requests.iter().all(|r| ![
                "/api/v2/priorities",
                "/api/v2/projects/2/issueTypes",
                "/api/v2/projects/2/versions"
            ]
            .contains(&r["path"].as_str().unwrap())),
            "{requests:?}"
        );
        assert!(
            result["snapshots"]
                .as_array()
                .unwrap()
                .iter()
                .any(|s| s.as_str().unwrap().contains("Updated: B-1")),
            "{result}"
        );
        if body {
            let record: Value =
                serde_json::from_slice(&std::fs::read(h.root.join("editor-record")).unwrap())
                    .unwrap();
            assert_eq!(record["original"], "", "Comment must not copy description");
        }
    }
}

#[test]
fn bl_comment_add_empty_and_same_values_reject_without_write_or_discard_and_never_copy_description()
{
    let h = Harness::new(comment_add_routes());
    settings(&h, "B");
    let result = scripted(
        &h,
        json!([
            {"wait":"OLD COMMENT","keys":"Llc"},
            {"wait":"Add comment","keys":"\t\t\t\t\r"},
            {"wait":"No body or fields have changed","keys":"\u{1b}"},
            {"wait":"> Comment","keys":"c"},
            {"wait":"Add comment","keys":"\t\r\r\t\rj\r\t\rjj\r\t\r"},
            {"wait":"No body or fields have changed","keys":"\u{1b}"},
            {"wait":"> Comment","keys":"q"}
        ]),
    );
    assert!(
        !result["stdout"]
            .as_str()
            .unwrap()
            .contains("Discard changes?")
    );
    assert!(h.requests().iter().all(|r| r["method"] == "GET"));
}

#[test]
fn bl_comment_add_write_failures_preserve_method_certainty_and_never_refresh_or_retry() {
    for patch in [false, true] {
        for (response, outcome) in [
            (
                json!({"status":403,"body":"{\"errors\":[{\"code\":5,\"message\":\"Denied test-secret\"}]}"}),
                if patch { "NotUpdated" } else { "NotCreated" },
            ),
            (json!({"status":500,"body":"{}"}), "Unknown"),
            (
                json!({"body":"not json"}),
                if patch {
                    "Updated; response processing failed"
                } else {
                    "Created; response processing failed"
                },
            ),
            (
                json!({"body":"{}"}),
                if patch {
                    "Updated; response processing failed"
                } else {
                    "Created; response processing failed"
                },
            ),
            (
                json!({"body":"{\"id\":99}","truncate":true}),
                if patch {
                    "Updated; response processing failed"
                } else {
                    "Created; response processing failed"
                },
            ),
        ] {
            let mut routes = comment_add_routes();
            let mut response = response;
            if response.get("status").is_none() {
                response["status"] = json!(if patch { 200 } else { 201 });
            }
            routes["routes"][if patch {
                "PATCH /api/v2/issues/B-1"
            } else {
                "POST /api/v2/issues/B-1/comments"
            }] = response;
            let h = Harness::new(routes);
            settings(&h, "B");
            h.config(json!({"editor":["/usr/bin/python3",format!("{}/tests/fixtures/editor.py",env!("CARGO_MANIFEST_DIR")),h.root.join("editor-record"),"body"],"profiles":{"default":{"spaceUrl":h.origin,"apiKeyEnv":"TEST_KEY"}}}));
            let result = scripted(
                &h,
                json!([
                    {"wait":"OLD COMMENT","keys":"Llc"},
                    {"wait":"Add comment","keys":"\r"},
                    {"wait":"EDITOR READY","keys":"\n"},
                    {"wait":"Body editor finished","keys":if patch {"\t\rj\r\t\t\t\r"} else {"\t\t\t\t\r"}},
                    {"wait":"Confirm comment","keys":"s"},
                    {"wait":outcome,"keys":"s\u{2}bad"},
                    {"wait":outcome,"keys":"\r"},
                    {"wait":"OLD COMMENT","keys":"q"}
                ]),
            );
            let requests = h.requests();
            let sent = requests.iter().position(|r| r["method"] != "GET").unwrap();
            assert_eq!(
                sent,
                requests.len() - 1,
                "No followup success GET or retries: {requests:?}"
            );
            assert!(!result["stdout"].as_str().unwrap().contains("test-secret"));
        }
    }
}

#[test]
fn bl_comment_add_post_and_patch_interrupt_abort_join_before_headers_after_headers_and_refresh() {
    for patch in [false, true] {
        for phase in ["sent", "headers", "refresh"] {
            for signal in [false, true] {
                let mut routes = comment_add_routes();
                let method = if patch {
                    "PATCH /api/v2/issues/B-1"
                } else {
                    "POST /api/v2/issues/B-1/comments"
                };
                if phase == "refresh" {
                    routes["routes"]["/api/v2/issues/B-1/comments"]["sequence"][1]["header_delay"] =
                        json!(20);
                } else {
                    routes["routes"][method][if phase == "sent" {
                        "header_delay"
                    } else {
                        "delay"
                    }] = json!(20);
                }
                let h = Harness::new(routes);
                settings(&h, "B");
                h.config(json!({"editor":["/usr/bin/python3",format!("{}/tests/fixtures/editor.py",env!("CARGO_MANIFEST_DIR")),h.root.join("editor-record"),"body"],"profiles":{"default":{"spaceUrl":h.origin,"apiKeyEnv":"TEST_KEY"}}}));
                let marker = format!(
                    "{}-{}",
                    if patch { "patch" } else { "post" },
                    if phase == "headers" {
                        "headers-sent"
                    } else {
                        "received"
                    }
                );
                let mut last = json!({"wait":if phase == "refresh" {"refreshing comments"} else {"Sending"},"wait_file":marker,"pause":0.15});
                if signal {
                    last["signal"] = json!("SIGINT");
                } else {
                    last["keys"] = json!("\u{3}");
                }
                std::fs::write(h.root.join("shell-steps.json"),json!([
                    {"wait":"OLD COMMENT","keys":"Llc"},
                    {"wait":"Add comment","keys":"\r"},
                    {"wait":"EDITOR READY","keys":"\n"},
                    {"wait":"Body editor finished","keys":if patch {"\t\rj\r\t\t\t\r"} else {"\t\t\t\t\r"}},
                    {"wait":"Confirm comment","keys":"s"},last
                ]).to_string()).unwrap();
                let result = shell(&h, "scripted", &[]);
                assert_eq!(result["code"], 130, "{patch}/{phase}/{signal}: {result}");
                assert_eq!(result["restored"], true, "{result}");
                assert!(result["elapsed"].as_f64().unwrap() < 5.0, "{result}");
                let stderr = result["stderr"].as_str().unwrap();
                assert!(
                    stderr.contains(if phase == "sent" {
                        "unknown:"
                    } else if patch {
                        "updated:"
                    } else {
                        "created:"
                    }),
                    "{result}"
                );
                assert!(
                    stderr.contains("comment.add") && stderr.contains("do not resend"),
                    "{result}"
                );
                assert_eq!(
                    h.requests().iter().filter(|r| r["method"] != "GET").count(),
                    1
                );
            }
        }
    }
}

#[test]
fn bl_comment_add_refresh_is_atomic_preserves_old_selection_scroll_and_certainty_on_failure() {
    for patch in [false, true] {
        for failure in ["http", "preview", "formatting", "empty", "body"] {
            let mut routes = comment_add_routes();
            let lines = (1..=40)
                .map(|n| format!("OLD LINE {n:02}"))
                .collect::<Vec<_>>()
                .join("\n");
            let old = json!([
                comment_item(
                    2,
                    Value::Null,
                    json!([{"field":"other","originalValue":null,"newValue":"OTHER ENTRY"}])
                ),
                comment_item(
                    1,
                    Value::Null,
                    json!([{"field":"old","originalValue":null,"newValue":lines}])
                )
            ]);
            routes["routes"]["/api/v2/issues/B-1/comments"]["sequence"][0] =
                json!({"body":old.to_string()});
            let fresh = match failure {
                "http" => json!({"status":500}),
                "preview" => {
                    json!({"body":json!([{"id":99,"content":"INVALID PREVIEW"}]).to_string()})
                }
                "empty" => json!({"body":"[]"}),
                _ => {
                    json!({"body":json!([comment_item(99,json!("FRESH BODY"),json!([]))]).to_string()})
                }
            };
            routes["routes"]["/api/v2/issues/B-1/comments"]["sequence"][1] = fresh;
            if failure == "formatting" {
                routes["routes"]["GET /api/v2/issues/B-1"]["sequence"][0]["header_delay"] =
                    json!(0.7);
                routes["routes"]["/api/v2/projects/2"] = json!({"sequence":[{"body":"{\"textFormattingRule\":\"markdown\"}"},{"body":"{\"textFormattingRule\":\"markdown\"}"},{"body":"{}"}]});
            }
            let h = Harness::new(routes);
            settings(&h, "B");
            h.config(json!({"editor":["/usr/bin/python3",format!("{}/tests/fixtures/editor.py",env!("CARGO_MANIFEST_DIR")),h.root.join("editor-record"),"body"],"profiles":{"default":{"spaceUrl":h.origin,"apiKeyEnv":"TEST_KEY"}}}));
            let success = matches!(failure, "empty" | "body");
            let result = scripted(
                &h,
                json!([
                    // Comment can load first; finish Detail so c cannot cancel its formatting GET.
                    {"wait":"TITLE: Detail B-1"},
                    {"wait":"OTHER ENTRY","keys":"Lln"},
                    {"wait":"OLD LINE 01","keys":"jjjjjc"},
                    {"wait":"Add comment","keys":"\r"},
                    {"wait":"EDITOR READY","keys":"\n"},
                    {"wait":"Body editor finished","keys":if patch {"\t\rj\r\t\t\t\r"} else {"\t\t\t\t\r"}},
                    {"wait":"Confirm comment","keys":"s"},
                    {"wait":if success {"Comment display refreshed"} else {"Comment R: refresh display"},"keys":"\r"},
                    {"wait":if failure == "empty" {"No comments"} else if success {"FRESH BODY"} else {"OLD LINE 06"},"keys":"q"}
                ]),
            );
            let result_screen = result["snapshots"][7].as_str().unwrap();
            assert!(
                result_screen.contains(if patch {
                    "Updated: B-1"
                } else {
                    "Created: B-1"
                }),
                "{failure}: {result}"
            );
            if !success {
                assert!(
                    result_screen.contains("Old display retained")
                        && result_screen.contains("Do not resend"),
                    "{result}"
                );
            }
            let requests = h.requests();
            let sent = requests.iter().position(|r| r["method"] != "GET").unwrap();
            if failure == "formatting" {
                let formatting_reads = |reads: &[Value]| {
                    reads
                        .iter()
                        .filter(|r| r["path"] == "/api/v2/projects/2")
                        .count()
                };
                assert_eq!(formatting_reads(&requests[..sent]), 2, "{requests:?}");
                assert_eq!(formatting_reads(&requests[sent + 1..]), 1, "{requests:?}");
            }
            assert_eq!(
                requests[sent + 1..]
                    .iter()
                    .filter(|r| r["path"] == "/api/v2/issues/B-1/comments?count=20&order=desc")
                    .count(),
                1,
                "{requests:?}"
            );
            assert!(requests[sent + 1..].iter().all(|r| {
                r["method"] == "GET"
                    && !r["path"].as_str().unwrap().starts_with("/api/v2/issues?")
                    && !r["path"]
                        .as_str()
                        .unwrap()
                        .starts_with("/api/v2/issues/count")
            }));
        }
    }
}

#[test]
fn bl_comment_add_editor_failures_resize_tiny_and_discard_keep_body_and_field_draft() {
    let h = Harness::new(comment_add_routes());
    settings(&h, "B");
    h.config(json!({"editor":["/usr/bin/python3",format!("{}/tests/fixtures/editor.py",env!("CARGO_MANIFEST_DIR")),h.root.join("editor-record"),"nonzero"],"profiles":{"default":{"spaceUrl":h.origin,"apiKeyEnv":"TEST_KEY"}}}));
    let result = scripted(
        &h,
        json!([
            {"wait":"OLD COMMENT","keys":"Llc"},
            {"wait":"Add comment","keys":"\t\rj\r\u{2}\r"},
            {"wait":"EDITOR READY","keys":"\n","resize":[12,40]},
            {"wait":"Editor did not exit successfully","resize":[11,39]},
            {"wait":"Terminal too small","keys":"s\r\u{1b}\u{2}"},
            {"wait":"Terminal too small","resize":[24,100]},
            {"wait":"Closed (2)","keys":"\t\t\t\t\r"},
            {"wait":"Confirm comment","keys":"\u{1b}"},
            {"wait":"Closed (2)","keys":"\u{1b}"},
            {"wait":"Discard changes?","keys":"\u{1b}"},
            {"wait":"Closed (2)","keys":"\u{1b}"},
            {"wait":"Discard changes?","keys":"\r"},
            {"wait":"> Comment","keys":"q"}
        ]),
    );
    let record: Value =
        serde_json::from_slice(&std::fs::read(h.root.join("editor-record")).unwrap()).unwrap();
    assert_eq!(record["original"], "");
    assert!(
        result["snapshots"][7]
            .as_str()
            .unwrap()
            .contains("Status: Closed (2)"),
        "{result}"
    );
    assert!(!std::path::Path::new(record["path"].as_str().unwrap()).exists());
    assert!(h.requests().iter().all(|r| r["method"] == "GET"));
}

#[test]
fn bl_comment_add_cancelled_load_and_reversed_detail_or_comment_reads_cannot_replace_new_draft_or_registration()
 {
    for comment_read in [false, true] {
        for failing in [false, true] {
            let mut routes = comment_add_routes();
            let delayed = if failing {
                json!({"header_delay":1.3,"status":403})
            } else {
                json!({"header_delay":1.3,"body":update_issue().to_string()})
            };
            let mut latest = update_issue();
            latest["assignee"] = json!({"id":9,"name":"New generation owner"});
            routes["routes"]["GET /api/v2/issues/B-1"]["sequence"] = if comment_read {
                json!([{"body":update_issue().to_string()},delayed.clone(),{"body":latest.to_string()}])
            } else {
                json!([{"body":update_issue().to_string()},delayed.clone(),delayed,{"body":latest.to_string()}])
            };
            if comment_read {
                let old = routes["routes"]["/api/v2/issues/B-1/comments"]["sequence"][0].clone();
                let fresh = routes["routes"]["/api/v2/issues/B-1/comments"]["sequence"][1].clone();
                routes["routes"]["/api/v2/issues/B-1/comments"]["sequence"] = json!([old,{"header_delay":1.3,"status":if failing {500} else {200},"body":"[]"},fresh]);
            }
            let h = Harness::new(routes);
            settings(&h, "B");
            let result = scripted_with_deadline(
                &h,
                json!([
                    // Finish both initial reads before reusing the generic GET marker.
                    {"wait":"TITLE: Detail B-1"},
                    {"wait":"OLD COMMENT","keys":if comment_read {"LlR"} else {"LR"},"remove":h.root.join("get-received")},
                    // A busy screen precedes HTTP arrival; wait for each request before cancelling.
                    {"wait":"Loading... Esc: cancel","wait_file":"get-received","keys":if comment_read {"c"} else {"lc"},"pause":0.2,"remove":h.root.join("get-received")},
                    {"wait":"Loading issue / candidates","wait_file":"get-received","keys":"\u{1b}","pause":0.2},
                    {"wait":"> Comment","keys":"c"},
                    {"wait":"New generation owner","keys":"\t\rj\r\t\t\t\r","pause":1.4},
                    {"wait":"Confirm comment","keys":"s"},
                    {"wait":"Comment display refreshed","keys":"\r"},
                    {"wait":"LATEST REGISTERED","keys":"q"}
                ]),
                // Whole multi-generation scenario budget, not an interrupt/response SLA.
                15,
            );
            let requests = h.requests();
            let writes: Vec<_> = requests.iter().filter(|r| r["method"] != "GET").collect();
            assert_eq!(writes.len(), 1, "{requests:?}");
            assert_eq!(writes[0]["method"], "PATCH");
            assert_eq!(writes[0]["path"], "/api/v2/issues/B-1");
            assert_eq!(writes[0]["form"], json!({"statusId":["2"]}));
            for (path, count) in [
                ("/api/v2/issues/B-1", if comment_read { 3 } else { 4 }),
                (
                    "/api/v2/issues/B-1/comments?count=20&order=desc",
                    if comment_read { 3 } else { 2 },
                ),
            ] {
                assert_eq!(
                    requests
                        .iter()
                        .filter(|r| r["method"] == "GET" && r["path"] == path)
                        .count(),
                    count,
                    "all old requests consumed before the new generation: {requests:?}"
                );
            }
            assert!(
                !result["snapshots"][5].as_str().unwrap().contains("ERROR"),
                "{result}"
            );
        }
    }
}

#[test]
fn bl_comment_edit_keeps_selected_id_and_page_sends_raw_content_once_without_refresh() {
    let h =
        Harness::new(serde_json::from_str(include_str!("fixtures/tui_comment_edit.json")).unwrap());
    let body = " Raw #23 日本語\n test-secret\u{1b}]52;c;unsafe\u{7}\n";
    settings(&h, "B");
    h.config(json!({"editor":["/usr/bin/python3",format!("{}/tests/fixtures/editor.py",env!("CARGO_MANIFEST_DIR")),h.root.join("editor-record"),"body-file"],"profiles":{"default":{"spaceUrl":h.origin,"apiKeyEnv":"TEST_KEY"}}}));
    std::fs::write(h.root.join("editor-body"), body).unwrap();
    let result = scripted(
        &h,
        serde_json::from_str(include_str!("fixtures/tui_comment_edit_steps.json")).unwrap(),
    );
    assert_eq!(result["code"], 0, "{result}");
    assert_eq!(result["restored"], true, "{result}");
    assert_eq!(
        result["request_counts"][4], result["request_counts"][5],
        "Enter sent: {result}"
    );
    let requests = h.requests();
    let writes: Vec<_> = requests.iter().filter(|r| r["method"] != "GET").collect();
    assert_eq!(writes.len(), 1, "{requests:?}");
    assert_eq!(writes[0]["path"], "/api/v2/issues/B-1/comments/1");
    assert_eq!(writes[0]["method"], "PATCH");
    assert_eq!(writes[0]["form"], json!({"content":[body]}));
    assert_eq!(
        requests.last().unwrap()["method"],
        "PATCH",
        "automatic GET: {requests:?}"
    );
    assert_eq!(
        requests
            .iter()
            .filter(|r| r["path"].as_str().unwrap().contains("comments?"))
            .count(),
        1
    );
    let record: Value =
        serde_json::from_slice(&std::fs::read(h.root.join("editor-record")).unwrap()).unwrap();
    assert_eq!(record["original"], "Latest raw body\n");
    for text in ["test-secret", "\u{1b}]52;"] {
        assert!(
            !result["stdout"].as_str().unwrap().contains(text),
            "{result}"
        );
    }
    assert!(
        result["snapshots"][11]
            .as_str()
            .unwrap()
            .contains("Open -> Closed"),
        "{result}"
    );
}

fn comment_edit_routes() -> Value {
    serde_json::from_str(include_str!("fixtures/tui_comment_edit.json")).unwrap()
}
fn comment_edit_editor(h: &Harness, mode: &str) {
    settings(h, "B");
    h.config(json!({"editor":["/usr/bin/python3",format!("{}/tests/fixtures/editor.py",env!("CARGO_MANIFEST_DIR")),h.root.join("editor-record"),mode],"profiles":{"default":{"spaceUrl":h.origin,"apiKeyEnv":"TEST_KEY"}}}));
}

#[test]
fn bl_comment_edit_refuses_latest_history_and_other_owner_before_editor_and_editor_failure_never_sends()
 {
    let mut routes = comment_edit_routes();
    let mut history = comment_item(1, Value::Null, json!([]));
    let mut other = comment_item(1, json!("Other person's body"), json!([]));
    other["createdUser"]["id"] = json!(8);
    history["createdUser"]["id"] = json!(7);
    let latest = routes["routes"]["GET /api/v2/issues/B-1/comments/1"].clone();
    routes["routes"]["GET /api/v2/issues/B-1/comments/1"] = json!({"sequence":[
        {"body":history.to_string()}, {"body":other.to_string()},latest
    ]});
    let h = Harness::new(routes);
    comment_edit_editor(&h, "nonzero");
    let result = scripted(
        &h,
        json!([
            {"wait":"Newest stays on page","keys":"Lln"},
            {"wait":"Displayed old body","keys":"e"},
            {"wait":"Change history has no editable comment body","keys":"\r"},
            {"wait":"Displayed old body","keys":"e"},
            {"wait":"Only your own comment body can be updated","keys":"\r"},
            {"wait":"Displayed old body","keys":"e"},
            {"wait":"EDITOR READY","keys":"\n"},
            {"wait":"Editor did not exit successfully","keys":"\r"},
            {"wait":"Comment body has not changed","keys":"\u{1b}"},
            {"wait":"Displayed old body","keys":"q"}
        ]),
    );
    assert_eq!(result["code"], 0, "{result}");
    assert_eq!(result["restored"], true, "{result}");
    assert!(h.requests().iter().all(|r| r["method"] == "GET"));
    assert!(
        !result["snapshots"][2]
            .as_str()
            .unwrap()
            .contains("EDITOR READY")
    );
    assert!(
        !result["snapshots"][4]
            .as_str()
            .unwrap()
            .contains("EDITOR READY")
    );
    let record: Value =
        serde_json::from_slice(&std::fs::read(h.root.join("editor-record")).unwrap()).unwrap();
    assert_eq!(record["original"], "Latest raw body\n");
    assert!(!std::path::Path::new(record["path"].as_str().unwrap()).exists());
}

#[test]
fn bl_comment_edit_rejection_and_wrong_target_success_keep_old_comment_without_retry_or_get() {
    for (response, outcome) in [
        (
            json!({"status":403,"body":"{\"errors\":[{\"code\":5,\"message\":\"Denied\"}]}"}),
            "not_updated: comment not updated",
        ),
        (
            json!({"body":comment_item(99,json!("WRONG TARGET"),json!([])).to_string()}),
            "updated: comment updated",
        ),
    ] {
        let mut routes = comment_edit_routes();
        routes["routes"]["PATCH /api/v2/issues/B-1/comments/1"] = response;
        let h = Harness::new(routes);
        comment_edit_editor(&h, "body");
        let result = scripted(
            &h,
            json!([
                {"wait":"Newest stays on page","keys":"Lln"},
                {"wait":"Displayed old body","keys":"e"},
                {"wait":"EDITOR READY","keys":"\n"},
                {"wait":"Body editor finished","keys":"\r"},
                {"wait":"Confirm comment update","keys":"s"},
                {"wait":outcome,"keys":"es"},
                {"wait":outcome,"keys":"\r","pause":0.2},
                {"wait":"Displayed old body","keys":"q"}
            ]),
        );
        assert_eq!(result["code"], 0, "{result}");
        assert_eq!(result["restored"], true, "{result}");
        let requests = h.requests();
        assert_eq!(
            requests.iter().filter(|r| r["method"] == "PATCH").count(),
            1,
            "{requests:?}"
        );
        assert_eq!(requests.last().unwrap()["method"], "PATCH", "{requests:?}");
        assert!(!result["stdout"].as_str().unwrap().contains("WRONG TARGET"));
    }
}

#[test]
fn bl_comment_edit_interrupt_aborts_joins_and_preserves_not_updated_unknown_updated() {
    for (phase, response, marker, outcome, signal) in [
        ("before", json!({"body":"{}"}), "", "not_updated", false),
        (
            "sent",
            json!({"header_delay":20,"body":"{}"}),
            "patch-received",
            "unknown",
            true,
        ),
        (
            "headers",
            json!({"delay":20,"body":"{}"}),
            "patch-headers-sent",
            "updated",
            false,
        ),
    ] {
        let mut routes = comment_edit_routes();
        routes["routes"]["PATCH /api/v2/issues/B-1/comments/1"] = response;
        let h = Harness::new(routes);
        comment_edit_editor(&h, "body");
        let mut last = json!({"wait":if phase=="before" {"Confirm comment update"} else {"Sending comment update"},"wait_file":marker,"pause":0.15});
        last[if signal { "signal" } else { "keys" }] =
            json!(if signal { "SIGINT" } else { "\u{3}" });
        let steps = json!([
            {"wait":"Newest stays on page","keys":"Lln"},
            {"wait":"Displayed old body","keys":"e"},
            {"wait":"EDITOR READY","keys":"\n"},
            {"wait":"Body editor finished","keys":"\r"},
            {"wait":"Confirm comment update","keys":if phase=="before" {""} else {"s"}},last
        ]);
        std::fs::write(h.root.join("shell-steps.json"), steps.to_string()).unwrap();
        let result = shell(&h, "scripted", &[]);
        assert_eq!(result["code"], 130, "{phase}: {result}");
        assert_eq!(result["restored"], true, "{result}");
        let stderr = result["stderr"].as_str().unwrap();
        assert!(stderr.contains(&format!("; {outcome}:")), "{result}");
        assert!(result["elapsed"].as_f64().unwrap() < 5.0, "{result}");
        assert_eq!(
            h.requests()
                .iter()
                .filter(|r| r["method"] == "PATCH")
                .count(),
            usize::from(phase != "before")
        );
    }
}

#[test]
fn bl_comment_edit_cancelled_load_and_late_comment_read_cannot_replace_new_edit_or_result() {
    let mut routes = comment_edit_routes();
    let latest = routes["routes"]["GET /api/v2/issues/B-1/comments/1"].clone();
    routes["routes"]["GET /api/v2/issues/B-1/comments/1"] = json!({"sequence":[
        {"header_delay":1.1,"body":comment_item(1,json!("CANCELLED LOAD"),json!([])).to_string()},latest
    ]});
    let page = routes["routes"]["/api/v2/issues/B-1/comments"].clone();
    routes["routes"]["/api/v2/issues/B-1/comments"] = json!({"sequence":[page,
        {"header_delay":0.9,"body":json!([comment_item(99,json!("LATE READ"),json!([]))]).to_string()}
    ]});
    let h = Harness::new(routes);
    comment_edit_editor(&h, "body-file");
    std::fs::write(
        h.root.join("editor-body"),
        " Raw #23 日本語\n test-secret\u{1b}]52;c;unsafe\u{7}\n",
    )
    .unwrap();
    let result = scripted(
        &h,
        json!([
            {"wait":"Newest stays on page","keys":"Lln"},
            {"wait":"Displayed old body","keys":"R"},
            {"wait":"Loading... Esc: cancel","keys":"e","wait_request":{"method":"GET","path":"/api/v2/issues/B-1/comments","ordinal":2}},
            {"wait":"Loading latest comment","keys":"\u{1b}","wait_request":{"method":"GET","path":"/api/v2/issues/B-1/comments/1","ordinal":1}},
            {"wait":"Displayed old body","keys":"e"},
            {"wait":"EDITOR READY","keys":"\n"},
            {"wait":"Body editor finished","keys":"\r"},
            {"wait":"Confirm comment update","keys":"s"},
            {"wait":"updated: comment updated","keys":"\r","pause":1.3},
            {"wait":"Raw #23","keys":"q"}
        ]),
    );
    assert_eq!(result["code"], 0, "{result}");
    for text in ["CANCELLED LOAD", "LATE READ", "Loading... Esc: cancel"] {
        assert!(
            !result["snapshots"][9].as_str().unwrap().contains(text),
            "{result}"
        );
    }
    assert_eq!(
        h.requests()
            .iter()
            .filter(|r| r["method"] == "PATCH")
            .count(),
        1
    );
}
