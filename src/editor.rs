//! Shell-free editor execution. The caller hands over and restores the terminal.
use crate::Error;
use std::{future::Future, io::Write, path::Path};
use tokio::process::Command;

/// Open an existing file, retaining saved changes even when the editor fails.
pub async fn edit_file(
    argv: &[String],
    path: &Path,
    interrupt: impl Future<Output = ()>,
) -> Result<(), Error> {
    if !path.is_file() {
        return Err(Error::new(
            "editor",
            &format!("Existing file required (not created): {}", path.display()),
        ));
    }
    let program = argv
        .first()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| Error::new("editor", "Editor program is missing"))?;
    let mut child = Command::new(program)
        .args(&argv[1..])
        .arg(path)
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| Error::new("editor", "Cannot run editor"))?;
    let status = tokio::select! {
        biased;
        () = interrupt => {
            let mut error = Error::new("interrupted", "Operation interrupted");
            if child.kill().await.is_err() {
                error.message.push_str("; Cannot stop/reap editor");
            }
            return Err(error);
        }
        status = child.wait() => status.map_err(|_| Error::new("editor", "Cannot wait for editor"))?,
    };
    if status.success() {
        Ok(())
    } else {
        Err(Error::new("editor", "Editor did not exit successfully"))
    }
}

/// Exchange the unredacted body only after editor, read and UTF-8 validation succeed.
pub async fn edit_body(
    argv: &[String],
    body: &mut String,
    interrupt: impl Future<Output = ()>,
) -> Result<(), Error> {
    let mut file = tempfile::NamedTempFile::new()
        .map_err(|_| Error::new("editor", "Cannot create private editor file"))?;
    let result = async {
        file.write_all(body.as_bytes())
            .map_err(|_| Error::new("editor", "Cannot write editor file"))?;
        file.flush()
            .map_err(|_| Error::new("editor", "Cannot flush editor file"))?;
        edit_file(argv, file.path(), interrupt).await?;
        std::fs::read_to_string(file.path())
            .map_err(|_| Error::new("editor", "Cannot read editor file as UTF-8"))
    }
    .await;
    let cleanup = file
        .close()
        .map_err(|_| Error::new("editor", "Cannot delete editor file"));
    match (result, cleanup) {
        (Ok(edited), Ok(())) => {
            *body = edited;
            Ok(())
        }
        (Err(mut error), Err(cleanup)) => {
            error.message.push_str(&format!("; {}", cleanup.message));
            Err(error)
        }
        (Err(error), _) | (_, Err(error)) => Err(error),
    }
}
