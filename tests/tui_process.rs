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
