use std::fs;
use std::process::{Command, Output, Stdio};

fn run_without_config(binary: &str, name: &str, flag: &str) -> Output {
    let directory =
        std::env::temp_dir().join(format!("backlog-cli-{}-{name}-{flag}", std::process::id()));
    fs::create_dir(&directory).expect("create empty configuration environment");
    let output = Command::new(binary)
        .arg(flag)
        .env_clear()
        .env("HOME", &directory)
        .env("XDG_CONFIG_HOME", &directory)
        .current_dir(&directory)
        .stdin(Stdio::null())
        .output();
    fs::remove_dir_all(&directory).expect("remove configuration environment");
    output.expect("run binary")
}

fn assert_help(binary: &str, name: &str) {
    for flag in ["--help", "-h"] {
        let output = run_without_config(binary, name, flag);
        assert_eq!(output.status.code(), Some(0));
        assert!(output.stderr.is_empty(), "{:?}", output);
        let stdout = String::from_utf8(output.stdout).expect("UTF-8 help");
        assert!(stdout.contains(&format!("Usage: {name}")), "{stdout}");
        assert!(stdout.contains("--help"), "{stdout}");
        assert!(stdout.contains("--version"), "{stdout}");
        assert!(
            !stdout.contains('\u{1b}'),
            "help must not contain ANSI escapes"
        );
    }
}

fn assert_version(binary: &str, name: &str, expected: &str) {
    for flag in ["--version", "-V"] {
        let output = run_without_config(binary, name, flag);
        assert_eq!(output.status.code(), Some(0));
        assert!(output.stderr.is_empty(), "{:?}", output);
        assert_eq!(
            String::from_utf8(output.stdout).expect("UTF-8 version"),
            expected
        );
    }
}

#[test]
fn bl_help_needs_neither_configuration_nor_a_terminal() {
    assert_help(env!("CARGO_BIN_EXE_bl"), "bl");
}

#[test]
fn bl_cli_help_needs_neither_configuration_nor_a_terminal() {
    assert_help(env!("CARGO_BIN_EXE_bl-cli"), "bl-cli");
}

#[test]
fn bl_version_needs_neither_configuration_nor_a_terminal() {
    assert_version(env!("CARGO_BIN_EXE_bl"), "bl", "bl 0.1.0\n");
}

#[test]
fn bl_cli_version_needs_neither_configuration_nor_a_terminal() {
    assert_version(env!("CARGO_BIN_EXE_bl-cli"), "bl-cli", "bl-cli 0.1.0\n");
}
