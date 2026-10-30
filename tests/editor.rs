use backlog_cli::editor;
use std::{fs, future::pending};

#[tokio::test]
async fn body_editor_receives_raw_private_file_and_literal_argv_then_replaces_and_deletes() {
    let record = std::env::temp_dir().join(format!("backlog-editor-record-{}", std::process::id()));
    let argv = vec![
        "/usr/bin/python3".into(), "-c".into(),
        "import pathlib,sys,os,stat; p=pathlib.Path(sys.argv[-1]); assert sys.argv[1]=='$(touch NEVER); *'; assert p.read_bytes()=='  原文 test-secret\\r\\n'.encode(); assert stat.S_IMODE(p.stat().st_mode)==0o600; assert p.stat().st_uid==os.getuid(); pathlib.Path(sys.argv[2]).write_text(str(p)); p.write_bytes(' 編集後\\n\\n'.encode())".into(),
        "$(touch NEVER); *".into(), record.to_string_lossy().into_owned(),
    ];
    let mut body = "  原文 test-secret\r\n".to_string();
    editor::edit_body(&argv, &mut body, pending())
        .await
        .unwrap();
    assert_eq!(body, " 編集後\n\n");
    let path = fs::read_to_string(&record).unwrap();
    assert!(!std::path::Path::new(&path).exists());
    fs::remove_file(record).unwrap();
}

#[tokio::test]
async fn body_interrupt_stops_and_reaps_editor_without_replacing_original() {
    let dir = tempfile::tempdir().unwrap();
    let record = dir.path().join("started");
    let argv = vec!["/usr/bin/python3".into(), "-c".into(),
        "import pathlib,sys,os,time; p=pathlib.Path(sys.argv[-1]); p.write_text('changed'); pathlib.Path(sys.argv[1]).write_text(str(os.getpid())+'\\n'+str(p)); time.sleep(2)".into(),
        record.to_string_lossy().into_owned()];
    let mut body = "original\n".to_string();
    let interrupt = async {
        while !record.exists() {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    };
    let error = editor::edit_body(&argv, &mut body, interrupt)
        .await
        .unwrap_err();
    assert_eq!(error.kind, "interrupted");
    assert_eq!(body, "original\n");
    let text = fs::read_to_string(record).unwrap();
    let (pid, path) = text.split_once('\n').unwrap();
    assert!(!std::path::Path::new(path).exists());
    let status = std::process::Command::new("/bin/kill")
        .args(["-0", pid])
        .output()
        .unwrap();
    assert!(!status.status.success(), "editor still running: {pid}");
}

#[tokio::test]
async fn body_failures_keep_original_and_delete_temporary_data() {
    for action in [
        "p.write_text('saved'); sys.exit(7)",
        "p.write_bytes(b'\\xff')",
        "p.unlink()",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let record = dir.path().join("path");
        let script = format!(
            "import pathlib,sys; p=pathlib.Path(sys.argv[-1]); pathlib.Path(sys.argv[1]).write_text(str(p)); {action}"
        );
        let argv = vec![
            "/usr/bin/python3".into(),
            "-c".into(),
            script,
            record.to_string_lossy().into_owned(),
        ];
        let mut body = "  原文\r\n".to_string();
        let error = editor::edit_body(&argv, &mut body, pending())
            .await
            .unwrap_err();
        assert_eq!(error.kind, "editor");
        assert_eq!(body, "  原文\r\n");
        assert!(!std::path::Path::new(&fs::read_to_string(record).unwrap()).exists());
    }
    let mut body = "original".to_string();
    assert!(
        editor::edit_body(&["/no/such/editor".into()], &mut body, pending())
            .await
            .is_err()
    );
    assert_eq!(body, "original");
}

#[tokio::test]
async fn existing_settings_keep_saved_changes_on_nonzero_and_missing_files_are_not_created() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    let argv = vec![
        "/usr/bin/python3".into(),
        "-c".into(),
        "import pathlib,sys; pathlib.Path(sys.argv[-1]).write_text('saved'); sys.exit(8)".into(),
    ];
    let error = editor::edit_file(&argv, &path, pending())
        .await
        .unwrap_err();
    assert!(error.message.contains("not created"));
    assert!(!path.exists());
    fs::write(&path, "original").unwrap();
    assert!(editor::edit_file(&argv, &path, pending()).await.is_err());
    assert_eq!(fs::read_to_string(path).unwrap(), "saved");
}

#[tokio::test]
async fn ready_parent_interrupt_takes_precedence_over_editor_exit() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    fs::write(&path, "original").unwrap();
    let error = editor::edit_file(&["/usr/bin/true".into()], &path, async {})
        .await
        .unwrap_err();
    assert_eq!(error.kind, "interrupted");
}
