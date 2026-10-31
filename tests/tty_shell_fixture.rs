mod common;
use common::Harness;
use serde_json::json;
use std::process::Command;

#[test]
fn tty_shell_deadline_reports_safe_progress_and_reaps_the_child() {
    driver_failure("deadline");
}

#[test]
fn tty_shell_exception_reports_safe_progress_and_reaps_the_child() {
    driver_failure("exception");
}

fn driver_failure(case: &str) {
    let h = Harness::new(json!({"routes": {
        "/api/v2/space": {"body":"{\"name\":\"Actual space screen-secret\"}"},
        "/api/v2/projects": {"body":"[]"}
    }}));
    let configured = h.command(&[]);
    let output = Command::new("python3")
        .args(["-E", "-S", "-c"])
        .arg(
            r#"
import http.client
import json
import os
import pathlib
import signal
import ssl
import subprocess
import sys
import time
import urllib.parse

root = pathlib.Path.cwd()
origin = urllib.parse.urlsplit(sys.argv[3])
context = ssl.create_default_context(cafile=os.environ["SSL_CERT_FILE"])
connection = http.client.HTTPSConnection(origin.hostname, origin.port, context=context, timeout=2)
try:
    for _ in range(2):
        connection.request("POST", "/diagnostic-probe?token=query-secret", body="body-secret",
                           headers={"Backlog-API-Key": "header-secret"})
        assert connection.getresponse().read() == b"[]"
finally:
    connection.close()
case = sys.argv[4]
steps = [{"wait": "No projects", "keys": "\t"},
         {"wait": "unreachable-step-secret", "keys": "unsent-keys-secret"}]
if case == "exception":
    steps[1] = {"wait": "", "keys": "unsent-keys-secret",
                "remove": str(root / "missing-path-secret")}
(root / "steps.json").write_text(json.dumps(steps))
env = dict(os.environ, SHELL_DEADLINE_SECONDS="1", SHELL_STEPS_FILE=str(root / "steps.json"),
           SHELL_REQUESTS_FILE=str(root / "requests"), PATH=str(root / "bin") + ":/usr/bin:/bin")
driver = subprocess.Popen(["python3", sys.argv[1], "scripted", sys.argv[2]],
                          env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                          text=True, start_new_session=True)
try:
    stdout, stderr = driver.communicate(timeout=5)
    assert driver.returncode == 1
    assert stdout == "", "failure must not dump success output or screens"
    for secret in ["query-secret", "body-secret", "header-secret", "test-secret", "screen-secret",
                   "unreachable-step-secret", "unsent-keys-secret", "missing-path-secret", "Traceback", "config.json"]:
        assert secret not in stderr, "failure diagnostics leaked " + secret
    report = json.loads(stderr)
    assert report["failure"] == ("deadline" if case == "deadline" else "FileNotFoundError")
    assert report["step_index"] == 1
    assert (1 if case == "deadline" else 0) <= report["elapsed"] < 3
    assert len(report["sent_at"]) == 1
    assert time.time() - 5 < report["sent_at"][0] <= time.time()
    assert report["request_counts"] == [4]
    assert report["request_count"] == 4
    assert report["requests"] == [
        {"method": "POST", "path": "/diagnostic-probe", "ordinal": 1},
        {"method": "POST", "path": "/diagnostic-probe", "ordinal": 2},
        {"method": "GET", "path": "/api/v2/space", "ordinal": 1},
        {"method": "GET", "path": "/api/v2/projects", "ordinal": 1},
    ]
    assert set(report) == {"failure", "step_index", "elapsed", "sent_at", "request_counts",
                           "request_count", "requests", "child_pid"}
    try:
        os.kill(report["child_pid"], 0)
    except ProcessLookupError:
        pass
    else:
        raise AssertionError("PTY child survived driver failure")
finally:
    # Also bound cleanup if a regression hangs the driver before it can reap bl.
    try:
        os.killpg(driver.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    driver.communicate(timeout=1)
"#,
        )
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/tty_shell.py"
        ))
        .arg(env!("CARGO_BIN_EXE_bl"))
        .arg(&h.origin)
        .arg(case)
        .env_clear()
        .envs(configured.get_envs().map(|(key, value)| (key, value.unwrap())))
        .env("PATH", std::env::var_os("PATH").unwrap())
        .current_dir(&h.root)
        .output()
        .expect("Cannot run PTY driver failure regression test");
    assert!(
        output.status.success(),
        "PTY driver {case} regression failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
