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

#[test]
fn tty_shell_waits_for_the_matching_request_ordinal_before_sending_input() {
    let h = Harness::new(json!({"body":"[]"}));
    let output = Command::new("python3")
        .args(["-E", "-S", "-c"])
        .arg(
            r#"
import json
import os
import pathlib
import signal
import subprocess
import sys

root = pathlib.Path.cwd()
(root / "child.py").write_text('''
import json, os, pathlib, select, sys, termios, tty
before = termios.tcgetattr(0)
try:
    tty.setraw(0)
    requests = pathlib.Path(os.environ["SHELL_REQUESTS_FILE"])
    requests.write_text("")
    print("\\x1b[1;1HREADY", end="", flush=True)
    def blocked():
        # A finite negative observation window: no input before the target receipt.
        assert not select.select([0], [], [], 0.15)[0], "input sent before matching receipt"
    def record(method, path):
        with requests.open("a") as destination:
            destination.write(json.dumps({"method": method, "path": path}) + "\\n")
    blocked()
    for _ in range(2):
        record("POST", "/api/v2/issues/B-1")
    blocked()
    for _ in range(2):
        record("GET", "/other?token=query-secret")
    blocked()
    record("GET", "/api/v2/issues/B-1")
    blocked()
    with requests.open("a") as destination:
        destination.write('{"method":')
    blocked()
    with requests.open("a") as destination:
        destination.write('"GET","path":"/api/v2/issues/B-1?token=query-secret"}\\n')
    assert select.select([0], [], [], 1)[0], "matching receipt did not release input"
    assert os.read(0, 1) == b"z"
    record("GET", "/after-input")
finally:
    termios.tcsetattr(0, termios.TCSANOW, before)
''')
(root / "steps.json").write_text(json.dumps([
    {"wait": "READY", "pause": 0, "wait_request":
        {"method": "GET", "path": "/api/v2/issues/B-1", "ordinal": 2}, "keys": "z"}
]))
env = dict(os.environ, SHELL_STEPS_FILE=str(root / "steps.json"),
           SHELL_REQUESTS_FILE=str(root / "requests"))
driver = subprocess.Popen([sys.executable, sys.argv[1], "scripted", sys.executable,
                           str(root / "child.py")], env=env, text=True,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
try:
    stdout, stderr = driver.communicate(timeout=5)
    assert driver.returncode == 0, stderr
    result = json.loads(stdout)
    assert result["code"] == 0, result
    assert result["stderr"] == "", result
    assert result["restored"], result
    assert len(result["snapshots"]) == len(result["sent_at"]) == 1, result
    assert result["request_counts"] == [6], result
    assert result["browse_request_counts"] == [3], result
    assert len((root / "requests").read_text().splitlines()) == 7
finally:
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
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .current_dir(&h.root)
        .output()
        .expect("Cannot run PTY request gate regression test");
    assert!(
        output.status.success(),
        "PTY request gate regression failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
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
    # Inject the exception without a pause for screen or input results.
    steps[1] = {"wait": "", "pause": 0, "keys": "unsent-keys-secret",
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
    assert report["failure"] == ("deadline" if case == "deadline" else "FileNotFoundError"), report
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
    assert report["terminal_size"] == {"rows": 24, "columns": 100}, report
    for field in ["screen_focus_markers", "raw_focus_markers_seen"]:
        assert set(report[field]) == {"project_list", "issue_list", "issue_detail", "comment"}, report
        assert all(type(value) is bool for value in report[field].values()), report
    if case == "deadline":
        assert report["screen_focus_markers"]["issue_list"] is True, report
        assert report["raw_focus_markers_seen"]["issue_list"] is True, report
    assert set(report) == {"failure", "step_index", "elapsed", "sent_at", "request_counts",
                           "request_count", "requests", "child_pid", "terminal_size",
                           "screen_focus_markers", "raw_focus_markers_seen"}
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
