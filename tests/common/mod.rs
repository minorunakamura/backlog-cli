#![allow(dead_code)]

use serde_json::{Value, json};
use std::fs;
use std::io::{BufRead, BufReader};
use std::os::unix::fs::DirBuilderExt;
use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

static NEXT: AtomicUsize = AtomicUsize::new(0);
pub struct Harness {
    pub root: PathBuf,
    server: Option<Child>,
    pub origin: String,
}
impl Harness {
    pub fn new(response: Value) -> Self {
        let root = std::env::temp_dir().join(format!(
            "backlog-project-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        // Own the directory before any fallible setup so startup failures also clean up.
        let mut harness = Self {
            root,
            server: None,
            origin: String::new(),
        };
        fs::create_dir(harness.root.join("backlog")).unwrap();
        fs::write(harness.root.join("response.json"), response.to_string()).unwrap();
        let started = Instant::now();
        harness.server = Some(
            Command::new("python3")
                .arg(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/https_server.py"
                ))
                .arg(&harness.root)
                .stdout(Stdio::piped())
                .spawn()
                .expect("Cannot start Python HTTPS fixture"),
        );
        let mut port = String::new();
        BufReader::new(harness.server.as_mut().unwrap().stdout.take().unwrap())
            .read_line(&mut port)
            .unwrap();
        let port: u16 = port
            .trim()
            .parse()
            .expect("HTTPS fixture did not report a port; see its stderr for setup errors");
        eprintln!(
            "[harness timing] spawn-to-ready={:.3}s",
            started.elapsed().as_secs_f64()
        );
        harness.origin = format!("https://localhost:{port}");
        harness.config(
            json!({"profiles":{"default":{"spaceUrl":harness.origin,"apiKeyEnv":"TEST_KEY"}}}),
        );
        harness
    }
    pub fn config(&self, value: Value) {
        fs::write(self.root.join("backlog/config.json"), value.to_string()).unwrap();
    }
    pub fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_bl-cli"));
        command
            .args(args)
            .env_clear()
            .env("HOME", &self.root)
            .env("XDG_CONFIG_HOME", &self.root)
            .env("TEST_KEY", "test-secret")
            .env("SSL_CERT_FILE", self.root.join("tls/localhost-cert.pem"))
            .env("NO_COLOR", "1")
            .current_dir(&self.root)
            .stdin(Stdio::null());
        command
    }
    pub fn run(&self, args: &[&str]) -> Output {
        self.command(args).output().unwrap()
    }
    pub fn requests(&self) -> Vec<Value> {
        fs::read_to_string(self.root.join("requests"))
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }
}
impl Drop for Harness {
    fn drop(&mut self) {
        let started = Instant::now();
        if let Some(server) = &mut self.server {
            let _ = server.kill();
            let _ = server.wait();
        }
        eprintln!(
            "[harness timing] stop={:.3}s",
            started.elapsed().as_secs_f64()
        );
        let started = Instant::now();
        fs::remove_dir_all(&self.root).unwrap();
        eprintln!(
            "[harness timing] cleanup={:.3}s",
            started.elapsed().as_secs_f64()
        );
    }
}
pub fn success(output: &Output) -> Value {
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    serde_json::from_slice(&output.stdout).unwrap()
}

pub fn failure(output: &Output, code: i32, kind: &str) -> Value {
    assert_eq!(output.status.code(), Some(code), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    let value: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(value["error"]["kind"], kind, "{value}");
    assert!(value["error"]["apiErrors"].is_array());
    assert!(value["error"].get("httpStatus").is_some());
    assert!(value["error"].get("write").is_none());
    assert_eq!(output.stderr.last(), Some(&b'\n'));
    value
}
