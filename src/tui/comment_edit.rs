use super::create::wrapped_lines;
use crate::{Error, WriteOutcome, config::Config, safety, service::Service};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    Frame,
    layout::Rect,
    widgets::{Block, Borders, Paragraph},
};
use serde_json::Value;
use std::cell::Cell;

#[derive(PartialEq)]
enum Phase {
    Loading,
    Form,
    Confirm,
    Discard,
    Sending,
    Result,
}

pub(super) struct Loaded {
    latest: Value,
    rule: String,
}
pub(super) async fn load(config: &Config, key: &str, id: &str) -> Result<Loaded, Error> {
    let service = Service::new(config)?;
    let latest = service.editable_comment(key, id).await?;
    let rule = service.issue_text_formatting_rule(key).await?;
    Ok(Loaded { latest, rule })
}

pub(super) struct Edit {
    pub(super) key: String,
    pub(super) id: String,
    pub(super) body: String,
    profile: String,
    origin: String,
    loaded: Option<Loaded>,
    phase: Phase,
    notice: String,
    scroll: Cell<usize>,
}
pub(super) enum Action {
    None,
    Close,
    Editor,
}
impl Edit {
    pub(super) fn new(key: String, id: String, profile: &str, origin: &str) -> Self {
        Self {
            key,
            id,
            body: String::new(),
            profile: profile.into(),
            origin: origin.into(),
            loaded: None,
            phase: Phase::Loading,
            notice: String::new(),
            scroll: Cell::new(0),
        }
    }
    pub(super) fn loading(&self) -> bool {
        self.phase == Phase::Loading
    }
    pub(super) fn sending(&self) -> bool {
        self.phase == Phase::Sending
    }
    pub(super) fn loaded(&mut self, result: Result<Loaded, Error>) -> bool {
        match result {
            Ok(loaded) => {
                self.body = loaded.latest["content"].as_str().unwrap().into();
                self.loaded = Some(loaded);
                self.phase = Phase::Form;
                true
            }
            Err(error) => {
                self.finish_error(error);
                false
            }
        }
    }
    pub(super) fn edited(&mut self, result: Result<(), Error>) {
        self.notice = match result {
            Ok(()) => "Body editor finished".into(),
            Err(error) => error.message,
        };
    }
    fn changed(&self) -> bool {
        self.loaded
            .as_ref()
            .is_some_and(|l| l.latest["content"].as_str() != Some(&self.body))
    }
    pub(super) fn key(&mut self, key: KeyEvent, area: Rect) -> Action {
        if key.kind != KeyEventKind::Press || !key.modifiers.is_empty() {
            return Action::None;
        }
        match self.phase {
            Phase::Loading if key.code == KeyCode::Esc => return Action::Close,
            Phase::Result if matches!(key.code, KeyCode::Esc | KeyCode::Enter) => {
                return Action::Close;
            }
            Phase::Discard => match key.code {
                KeyCode::Enter | KeyCode::Char(' ') => return Action::Close,
                KeyCode::Esc => self.phase = Phase::Form,
                _ => {}
            },
            Phase::Form => match key.code {
                KeyCode::Char('e') => return Action::Editor,
                KeyCode::Esc if self.changed() => self.phase = Phase::Discard,
                KeyCode::Esc => return Action::Close,
                KeyCode::Enter if self.body.is_empty() => {
                    self.notice = "Comment body must not be empty".into()
                }
                KeyCode::Enter if !self.changed() => {
                    self.notice = "Comment body has not changed".into()
                }
                KeyCode::Enter => {
                    self.phase = Phase::Confirm;
                    self.scroll.set(0);
                }
                _ => {}
            },
            Phase::Confirm => match key.code {
                KeyCode::Char('s') => self.phase = Phase::Sending,
                KeyCode::Esc => {
                    self.phase = Phase::Form;
                    self.scroll.set(0);
                }
                _ => {}
            },
            _ => {}
        }
        let page = area.height.saturating_sub(4).max(1) as usize;
        self.scroll.set(match key.code {
            KeyCode::Down | KeyCode::Char('j') => self.scroll.get().saturating_add(1),
            KeyCode::Up | KeyCode::Char('k') => self.scroll.get().saturating_sub(1),
            KeyCode::PageDown | KeyCode::Char(' ') if self.phase != Phase::Discard => {
                self.scroll.get().saturating_add(page)
            }
            KeyCode::PageUp | KeyCode::Char('b') => self.scroll.get().saturating_sub(page),
            KeyCode::Home | KeyCode::Char('g') => 0,
            KeyCode::End | KeyCode::Char('G') => usize::MAX,
            _ => self.scroll.get(),
        });
        Action::None
    }
    pub(super) fn submission(&self) -> (String, String, String, Value) {
        (
            self.key.clone(),
            self.id.clone(),
            self.body.clone(),
            self.loaded.as_ref().unwrap().latest.clone(),
        )
    }
    pub(super) fn finished(
        &mut self,
        result: Result<Value, Error>,
        config: &Config,
    ) -> Option<crate::comment::Preview> {
        match result.and_then(|value| {
            crate::comment::preview(&value, &self.loaded.as_ref().unwrap().rule, config)
                .map_err(|error| error.with_write_outcome("comment.update", WriteOutcome::Updated))
        }) {
            Ok(preview) => {
                self.phase = Phase::Result;
                self.notice = "updated: comment updated; do not resend".into();
                self.scroll.set(0);
                Some(preview)
            }
            Err(error) => {
                self.finish_error(error);
                None
            }
        }
    }
    fn finish_error(&mut self, error: Error) {
        let outcome = error
            .write
            .as_ref()
            .map(|w| w.outcome)
            .unwrap_or(WriteOutcome::NotUpdated);
        let label = match outcome {
            WriteOutcome::Updated => "updated: comment updated; do not resend",
            WriteOutcome::Unknown => "unknown: comment update result unknown; do not resend",
            _ => "not_updated: comment not updated",
        };
        self.notice = format!("{label}\n{}", error.message);
        self.phase = Phase::Result;
        self.scroll.set(0);
    }
    pub(super) fn draw(&self, frame: &mut Frame<'_>, secret: &str) {
        let area = frame.area();
        let title = match self.phase {
            Phase::Loading => "Loading comment edit",
            Phase::Form => "Edit comment body",
            Phase::Confirm => "Confirm comment update (no PATCH yet)",
            Phase::Discard => "Discard changes?",
            Phase::Sending => "Sending comment update",
            Phase::Result => "Comment update result",
        };
        frame.render_widget(Block::default().borders(Borders::ALL).title(title), area);
        let inner = Rect::new(1, 1, area.width - 2, area.height - 2);
        let content = match self.phase {
            Phase::Loading => "Loading latest comment / myself... Esc: cancel".into(),
            Phase::Discard => "Discard changes? Enter/Space: discard  Esc: keep editing".into(),
            Phase::Result => self.notice.clone(),
            _ => format!(
                "Profile: {}\nOrigin: {}\nIssue: {}\nComment ID: {}\nIssue fields: unchanged\nBody:\n{}\n\n{}",
                self.profile,
                self.origin,
                self.key,
                self.id,
                self.body,
                if self.phase == Phase::Form {
                    &self.notice
                } else {
                    ""
                }
            ),
        };
        let safe = safety::text(&content, &[secret]);
        let body = Rect::new(inner.x, inner.y, inner.width, inner.height - 2);
        let lines = wrapped_lines(&safe, body.width, &[secret]);
        let offset = self
            .scroll
            .get()
            .min(lines.len().saturating_sub(body.height as usize));
        self.scroll.set(offset);
        frame.render_widget(
            Paragraph::new(
                lines
                    .into_iter()
                    .skip(offset)
                    .take(body.height as usize)
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
            body,
        );
        let help = match self.phase {
            Phase::Form => "e: editor  Enter: confirm  Esc: cancel\nj/k PgUp/Dn Home/End: read all",
            Phase::Confirm => {
                "s SEND ONCE  Esc: edit (Enter: no send)\nj/k PgUp/Dn Home/End: read all"
            }
            Phase::Sending => "Sending: edit/send/discard disabled\nCtrl+C: interrupt",
            Phase::Result => "Enter/Esc: Comment\nj/k PgUp/Dn Home/End: read all",
            _ => "Ctrl+C: interrupt",
        };
        frame.render_widget(
            Paragraph::new(help),
            Rect::new(inner.x, inner.y + inner.height - 2, inner.width, 2),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::Shell;
    use crossterm::event::KeyModifiers;
    use ratatui::{Terminal, backend::TestBackend};
    use serde_json::json;

    #[test]
    fn shell_comment_edit_requires_a_changed_nonempty_body_and_guards_discard_and_tiny_confirmation()
     {
        let mut shell = Shell::new(true);
        shell.comment_edit = Some(Edit::new(
            "B-1".into(),
            "1".into(),
            "work",
            "https://example.com/",
        ));
        shell.comment_edit.as_mut().unwrap().loaded(Ok(Loaded {
            latest: json!({"content":"original"}),
            rule: "markdown".into(),
        }));
        let area = Rect::new(0, 0, 100, 24);
        let key = |shell: &mut Shell, code, area| {
            shell.key(KeyEvent::new(code, KeyModifiers::NONE), area);
        };
        let screen = |shell: &Shell| {
            let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
            terminal.draw(|frame| shell.draw(frame)).unwrap();
            terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|c| c.symbol())
                .collect::<String>()
        };
        key(&mut shell, KeyCode::Enter, area);
        assert!(screen(&shell).contains("Comment body has not changed"));
        // The shared editor keeps the original on failure; the form must not make it sendable.
        shell
            .comment_edit
            .as_mut()
            .unwrap()
            .edited(Err(Error::new("editor", "Editor failed")));
        key(&mut shell, KeyCode::Enter, area);
        assert!(screen(&shell).contains("Comment body has not changed"));
        shell.comment_edit.as_mut().unwrap().body.clear();
        key(&mut shell, KeyCode::Enter, area);
        assert!(screen(&shell).contains("Comment body must not be empty"));
        shell.comment_edit.as_mut().unwrap().body = "changed".into();
        key(&mut shell, KeyCode::Esc, area);
        assert!(screen(&shell).contains("Discard changes?"));
        key(&mut shell, KeyCode::Esc, area);
        key(&mut shell, KeyCode::Enter, Rect::new(0, 0, 39, 11));
        assert!(screen(&shell).contains("Edit comment body"));
        key(&mut shell, KeyCode::Enter, area);
        for code in [KeyCode::Enter, KeyCode::Char('s')] {
            key(&mut shell, code, Rect::new(0, 0, 39, 11));
        }
        assert!(screen(&shell).contains("Confirm comment update"));
        for text in [
            "Profile: work",
            "Origin: https://example.com/",
            "Issue: B-1",
            "Comment ID: 1",
            "Issue fields: unchanged",
            "changed",
        ] {
            assert!(screen(&shell).contains(text), "{text}");
        }
        key(&mut shell, KeyCode::Enter, area);
        assert!(screen(&shell).contains("Confirm comment update"));
        key(&mut shell, KeyCode::Esc, area);
        key(&mut shell, KeyCode::Esc, area);
        key(&mut shell, KeyCode::Enter, area);
        assert!(!screen(&shell).contains("Edit comment body"));
    }
}
