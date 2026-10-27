use std::process::Command;

#[test]
fn local_https_fixture_starts_and_serves_without_reverse_dns() {
    let output = Command::new("python3")
        .args(["-E", "-S", "-c"])
        .arg(
            r#"
import http.client
import pathlib
import select
import ssl
import subprocess
import sys
import tempfile

# Fail at the DNS boundary rather than relying on a machine's resolver latency.
child_code = '''
import runpy, socket, sys
def no_reverse_dns(*args, **kwargs):
    raise AssertionError("loopback fixture must not require reverse DNS")
socket.getfqdn = no_reverse_dns
sys.argv = sys.argv[1:]
runpy.run_path(sys.argv[0], run_name="__main__")
'''
with tempfile.TemporaryDirectory(prefix="backlog-no-reverse-dns-") as root:
    pathlib.Path(root, "response.json").write_text('{"body":"[]"}')
    child = subprocess.Popen(
        [sys.executable, "-E", "-S", "-c", child_code, sys.argv[1], root],
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
    )
    try:
        assert select.select([child.stdout], [], [], 10)[0], "fixture startup timed out"
        port = child.stdout.readline().strip()
        if not port.isdigit():
            child.kill()
            _, errors = child.communicate(timeout=5)
            raise AssertionError(errors or "fixture did not report a port")
        context = ssl.create_default_context(cafile=str(pathlib.Path(root, "tls/localhost-cert.pem")))
        connection = http.client.HTTPSConnection("localhost", int(port), context=context, timeout=5)
        try:
            connection.request("GET", "/api/v2/projects")
            response = connection.getresponse()
            assert response.status == 200
            assert response.read() == b"[]"
        finally:
            connection.close()
    finally:
        child.kill()
        _, timings = child.communicate(timeout=5)
    for phase in ["certificate", "tls-context", "server-init", "tls-wrap"]:
        assert phase + "=" in timings, "missing fixture timing: " + phase
"#,
        )
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/https_server.py"
        ))
        .output()
        .expect("Cannot run Python HTTPS fixture regression test");
    assert!(
        output.status.success(),
        "HTTPS fixture regression failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
