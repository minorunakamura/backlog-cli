//! Read-only project, issue-list and detail browsing.
mod browse;
mod comment;
mod detail;
mod input;
mod search;
use crate::{
    Error,
    config::{self, Config},
    editor, safety,
};
use crossterm::{
    cursor::{Hide, Show},
    event::{
        self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers,
    },
    execute,
    style::{Attribute, SetAttribute},
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
pub use input::SingleLineInput;
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
};
use std::{
    io::{self, Write},
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
    time::Duration,
};
use tokio::runtime::Builder;

#[derive(Clone, Copy, PartialEq)]
enum Focus {
    Project,
    Issue,
    Detail,
    Comment,
}

enum EditorRequest {
    Settings(usize),
    #[cfg(test)]
    Body,
}

/// Input and rendering boundary, independent of the terminal and network.
pub struct Shell {
    focus: Focus,
    left_focus: Focus,
    right_focus: Focus,
    browse: browse::Browse,
    no_color: bool,
    settings_selection: Option<usize>,
    editor_request: Option<EditorRequest>,
    #[cfg(test)]
    input_check: Option<tests::InputCheck>,
}
impl Shell {
    pub fn new(no_color: bool) -> Self {
        Self {
            focus: Focus::Project,
            left_focus: Focus::Project,
            right_focus: Focus::Detail,
            browse: browse::Browse::default(),
            no_color,
            settings_selection: None,
            editor_request: None,
            #[cfg(test)]
            input_check: None,
        }
    }
    /// Return the requested process exit code; tiny terminals accept only safe exits.
    pub fn key(&mut self, key: KeyEvent, area: Rect) -> Option<u8> {
        if key.kind == KeyEventKind::Release {
            return None;
        }
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return Some(130);
        }
        #[cfg(test)]
        if let Some(input) = &mut self.input_check {
            if area.width >= 40
                && area.height >= 12
                && !input.focused
                && input.editor_body.is_some()
                && key.code == KeyCode::Char('e')
                && key.modifiers.is_empty()
            {
                self.browse.error_notice = None;
                self.editor_request = Some(EditorRequest::Body);
                return None;
            }
            if self.browse.error_notice.is_some()
                && matches!(key.code, KeyCode::Esc | KeyCode::Enter)
            {
                self.browse.error_notice = None;
                return None;
            }
            return input.key(key, area);
        }
        if self.browse.dialog.is_some() && area.width >= 40 && area.height >= 12 {
            self.browse.error_notice = None;
            self.browse.dialog_event(&Event::Key(key), area);
            return None;
        }
        if let Some(selected) = self.settings_selection.as_mut()
            && area.width >= 40
            && area.height >= 12
        {
            if !key.modifiers.is_empty() {
                return None;
            }
            match key.code {
                KeyCode::Char('j' | 'k') | KeyCode::Up | KeyCode::Down => *selected = 1 - *selected,
                KeyCode::Enter => {
                    self.editor_request = Some(EditorRequest::Settings(*selected));
                    self.settings_selection = None;
                }
                KeyCode::Esc => self.settings_selection = None,
                _ => {}
            }
            return None;
        }
        if key.modifiers.is_empty() && key.code == KeyCode::Char('q') {
            return Some(0);
        }
        if area.width < 40 || area.height < 12 {
            return None;
        }
        if key.code == KeyCode::Char('*')
            && (key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT)
        {
            self.browse.error_notice = None;
            self.settings_selection = Some(0);
            return None;
        }
        if matches!(key.code, KeyCode::Char('H' | 'L')) {
            if key.kind == KeyEventKind::Press
                && (key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT)
            {
                self.browse.error_notice = None;
                match (self.focus, key.code) {
                    (Focus::Project | Focus::Issue, KeyCode::Char('L')) => {
                        self.set_focus(self.right_focus);
                    }
                    (Focus::Detail | Focus::Comment, KeyCode::Char('H')) => {
                        self.set_focus(self.left_focus);
                    }
                    _ => {}
                }
            }
            return None;
        }
        if self.browse.error_notice.is_some()
            && key.modifiers.is_empty()
            && matches!(key.code, KeyCode::Enter | KeyCode::Esc | KeyCode::Char(' '))
        {
            self.browse.error_notice = None;
            return None;
        }
        if key.modifiers == KeyModifiers::CONTROL
            && key.code == KeyCode::Char('a')
            && self.focus == Focus::Project
        {
            self.browse.error_notice = None;
            self.browse.open_search(true);
            return None;
        }
        if key.modifiers.is_empty() && key.code == KeyCode::Char('/') && self.focus == Focus::Issue
        {
            self.browse.error_notice = None;
            self.browse.open_search(false);
            return None;
        }
        if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('o') {
            self.browse.error_notice = None;
            if self.focus == Focus::Issue {
                self.browse.open_browser();
            }
            return None;
        }
        if !key.modifiers.is_empty() && key.modifiers != KeyModifiers::SHIFT {
            return None;
        }
        self.browse.error_notice = None;
        let panel = self.panel_area(area, self.focus);
        let rows = panel.height.saturating_sub(2) as usize;
        if matches!(self.focus, Focus::Detail | Focus::Comment)
            && !(self.focus == Focus::Comment && matches!(key.code, KeyCode::Char('g' | 'G')))
        {
            let body_area = Self::body_area(panel, self.focus);
            let text = if self.focus == Focus::Comment {
                self.browse
                    .comment
                    .projection(body_area.width, self.no_color, &self.browse.secret)
                    .2
            } else {
                None
            };
            let document = if self.focus == Focus::Detail {
                &mut self.browse.detail
            } else {
                &mut self.browse.comment.document
            };
            document.key(key.code, body_area, &self.browse.secret, text);
        }
        self.browse.key(key.code, self.focus, rows);
        match key.code {
            KeyCode::Char('h' | 'l') | KeyCode::Tab | KeyCode::BackTab => {
                self.set_focus(match self.focus {
                    Focus::Project => Focus::Issue,
                    Focus::Issue => Focus::Project,
                    Focus::Detail => Focus::Comment,
                    Focus::Comment => Focus::Detail,
                });
            }
            KeyCode::Enter if self.focus == Focus::Issue => self.set_focus(Focus::Detail),
            KeyCode::Esc if matches!(self.focus, Focus::Detail | Focus::Comment) => {
                self.set_focus(Focus::Issue)
            }
            _ => {}
        }
        None
    }
    fn set_focus(&mut self, focus: Focus) {
        self.focus = focus;
        match focus {
            Focus::Project | Focus::Issue => self.left_focus = focus,
            Focus::Detail | Focus::Comment => self.right_focus = focus,
        }
    }
    fn event(&mut self, event: Event, area: Rect) -> Option<u8> {
        match event {
            Event::Key(key) => self.key(key, area),
            Event::Paste(_) | Event::Resize(..)
                if self.browse.dialog.is_some() && area.width >= 40 && area.height >= 12 =>
            {
                self.browse.dialog_event(&event, area);
                None
            }
            #[cfg(test)]
            Event::Paste(_) if self.input_check.as_ref().is_some_and(|input| input.focused) => {
                self.input_check.as_mut().unwrap().field.event(&event, area);
                None
            }
            _ => None,
        }
    }
    pub fn draw(&self, frame: &mut Frame<'_>) {
        let area = frame.area();
        if area.width < 40 || area.height < 12 {
            frame.render_widget(
                Paragraph::new("Terminal too small: need 40x12. q: quit / Ctrl+C: interrupt")
                    .wrap(Wrap { trim: false }),
                area,
            );
            return;
        }
        #[cfg(test)]
        if let Some(input) = &self.input_check {
            input.draw(frame);
            self.error_popup(frame);
            return;
        }
        let compact = area.width < 100 || area.height < 24;
        let rows = Layout::vertical([
            Constraint::Min(0),
            Constraint::Length(if compact { 3 } else { 1 }),
        ])
        .split(area);
        let (navigation, action) = match self.focus {
            Focus::Project => ("h/l Tab/S-Tab:Issue", "j/k Enter:select Ctrl+A:cross R"),
            Focus::Issue => ("h/l Tab/S-Tab:Project", "j/k p/n Enter:Detail / Ctrl+O R"),
            Focus::Detail => ("h/l Tab/S-Tab:Comment", "j/k PgUp/Dn g/G R Esc:Issue"),
            Focus::Comment => (
                "h/l Tab/S-Tab:Detail",
                "j/k n:older p:newer g/G R Esc:Issue",
            ),
        };
        let regions = "H:Left L:Right";
        let exits = if compact {
            "*:settings q:quit Ctrl+C:interrupt"
        } else {
            "q:quit Ctrl+C:interrupt"
        };
        let keys = if self.browse.dialog.is_some() {
            if compact {
                "\n\nEsc: close/cancel Ctrl+C: interrupt".into()
            } else {
                "Search | Esc: close candidates / cancel draft | Ctrl+C: interrupt".into()
            }
        } else if compact {
            format!("{regions} {navigation}\n{action}\n{exits}")
        } else {
            format!("{regions} | {navigation} | {action} | {exits}")
        };
        frame.render_widget(Paragraph::new(keys), rows[1]);
        if compact {
            self.panel(frame, rows[0], self.focus);
            if let Some(dialog) = &self.browse.dialog {
                dialog.draw(frame, &[&self.browse.secret]);
            }
            self.settings_popup(frame);
            self.error_popup(frame);
            return;
        }
        let main = Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).split(rows[0]);
        let workspace = format!(
            "{}  | * settings",
            self.browse.space.as_deref().unwrap_or("Loading space...")
        );
        frame.render_widget(
            Paragraph::new(workspace)
                .block(Block::default().borders(Borders::ALL).title("Workspace")),
            main[0],
        );
        for focus in [Focus::Project, Focus::Issue, Focus::Detail, Focus::Comment] {
            self.panel(frame, self.panel_area(area, focus), focus);
        }
        if let Some(dialog) = &self.browse.dialog {
            dialog.draw(frame, &[&self.browse.secret]);
        }
        self.settings_popup(frame);
        self.error_popup(frame);
    }
    fn settings_popup(&self, frame: &mut Frame<'_>) {
        let Some(selected) = self.settings_selection else {
            return;
        };
        let area = frame.area();
        let width = area.width.saturating_sub(4).min(56);
        let popup = Rect::new(
            area.x + (area.width - width) / 2,
            area.y + (area.height - 6) / 2,
            width,
            6,
        );
        frame.render_widget(Clear, popup);
        let text = format!(
            "{} Common config\n{} cwd settings\nj/k Up/Down Enter:open Esc:cancel\nChanges apply after restart",
            if selected == 0 { ">" } else { " " },
            if selected == 1 { ">" } else { " " }
        );
        frame.render_widget(
            Paragraph::new(text).block(Block::bordered().title("Settings editor")),
            popup,
        );
    }
    fn error_popup(&self, frame: &mut Frame<'_>) {
        let Some(message) = &self.browse.error_notice else {
            return;
        };
        let area = frame.area();
        let width = area.width.saturating_sub(4).min(76);
        let mut style = Style::default().add_modifier(Modifier::BOLD);
        if !self.no_color {
            style = style.fg(Color::Red);
        }
        let paragraph = Paragraph::new(message.as_str())
            .wrap(Wrap { trim: false })
            .style(style);
        let height = paragraph
            .line_count(width.saturating_sub(2))
            .saturating_add(2)
            .min(area.height.saturating_sub(2) as usize) as u16;
        let popup = Rect::new(
            area.x + (area.width - width) / 2,
            area.y + (area.height - height) / 2,
            width,
            height,
        );
        frame.render_widget(Clear, popup);
        frame.render_widget(
            paragraph.block(Block::default().borders(Borders::ALL)),
            popup,
        );
    }
    fn panel_area(&self, area: Rect, focus: Focus) -> Rect {
        let compact = area.width < 100 || area.height < 24;
        let rows = Layout::vertical([
            Constraint::Min(0),
            Constraint::Length(if compact { 3 } else { 1 }),
        ])
        .split(area);
        if compact {
            return rows[0];
        }
        let main = Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).split(rows[0]);
        let columns = Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(main[1]);
        match focus {
            Focus::Project | Focus::Issue => {
                Layout::vertical([Constraint::Percentage(35), Constraint::Min(0)]).split(columns[0])
                    [if focus == Focus::Project { 0 } else { 1 }]
            }
            Focus::Detail | Focus::Comment => Layout::vertical([
                Constraint::Length(
                    columns[1]
                        .height
                        .saturating_sub((columns[1].height / 5).max(6)),
                ),
                Constraint::Min(0),
            ])
            .split(columns[1])[if focus == Focus::Detail { 0 } else { 1 }],
        }
    }
    fn body_area(panel: Rect, focus: Focus) -> Rect {
        let inner = Block::default().borders(Borders::ALL).inner(panel);
        let fixed = if focus == Focus::Comment { 3 } else { 1 };
        Rect::new(
            inner.x,
            inner.y + fixed.min(inner.height),
            inner.width,
            inner.height.saturating_sub(fixed),
        )
    }
    fn panel(&self, frame: &mut Frame<'_>, area: Rect, focus: Focus) {
        let title = match focus {
            Focus::Project => "Project List",
            Focus::Issue => "Issue List",
            Focus::Detail => "Issue Detail",
            Focus::Comment => "Comment",
        };
        let title = if focus == Focus::Comment {
            format!("{title} {}", self.browse.comment_status().trim())
                .trim_end()
                .to_owned()
        } else {
            title.into()
        };
        let label = if focus == self.focus {
            format!("> {title}")
        } else {
            title
        };
        let mut block = Block::default().borders(Borders::ALL).title(label);
        if focus == self.focus {
            let mut style = Style::default().add_modifier(Modifier::BOLD);
            if !self.no_color {
                style = style.fg(Color::Cyan);
            }
            block = block.border_type(BorderType::Double).border_style(style);
        }
        if matches!(focus, Focus::Detail | Focus::Comment) {
            let inner = block.inner(area);
            frame.render_widget(block, area);
            let body_area = Self::body_area(area, focus);
            if focus == Focus::Comment {
                let (author, created, text) =
                    self.browse
                        .comment
                        .projection(inner.width, self.no_color, &self.browse.secret);
                let mut style = Style::default().add_modifier(Modifier::BOLD);
                if !self.no_color {
                    style = style.fg(Color::Cyan);
                }
                frame.render_widget(
                    Paragraph::new(author).style(style),
                    Rect::new(inner.x, inner.y, inner.width, 1.min(inner.height)),
                );
                frame.render_widget(
                    Paragraph::new(created),
                    Rect::new(
                        inner.x,
                        inner.y + 1.min(inner.height),
                        inner.width,
                        1.min(inner.height.saturating_sub(1)),
                    ),
                );
                self.browse
                    .comment
                    .document
                    .draw(frame, body_area, &self.browse.secret, text);
            } else {
                frame.render_widget(
                    Paragraph::new(self.browse.detail_status()),
                    Rect::new(inner.x, inner.y, inner.width, 1.min(inner.height)),
                );
                self.browse
                    .detail
                    .draw(frame, body_area, &self.browse.secret, None);
            }
            return;
        }
        let content = self
            .browse
            .content(focus, area.height.saturating_sub(2) as usize);
        frame.render_widget(Paragraph::new(content).block(block), area);
    }
}

struct TerminalGuard<W: Write> {
    output: W,
    original_termios: rustix::termios::Termios,
    active: bool,
}
impl<W: Write> TerminalGuard<W> {
    fn enter(&mut self) -> Result<(), Error> {
        // Mark before the first fallible operation so partial startup is restored too.
        self.active = true;
        enable_raw_mode().map_err(|_| Error::new("terminal", "Cannot enable raw mode"))?;
        execute!(self.output, EnterAlternateScreen)
            .map_err(|_| Error::new("terminal", "Cannot enter alternate screen"))?;
        execute!(self.output, Hide).map_err(|_| Error::new("terminal", "Cannot hide cursor"))?;
        execute!(self.output, EnableBracketedPaste)
            .map_err(|_| Error::new("terminal", "Cannot enable bracketed paste"))
    }
    fn restore(&mut self) -> Result<(), Error> {
        if !self.active {
            return Ok(());
        }
        self.active = false;
        let mut failed = vec![];
        if disable_raw_mode().is_err() {
            failed.push("raw mode");
        }
        // The editor may leave raw/noecho behind; Crossterm's snapshot ends at handoff.
        if rustix::termios::tcsetattr(
            io::stdin(),
            rustix::termios::OptionalActions::Now,
            &self.original_termios,
        )
        .is_err()
        {
            failed.push("termios");
        }
        if execute!(self.output, DisableBracketedPaste).is_err() {
            failed.push("bracketed paste");
        }
        if execute!(self.output, SetAttribute(Attribute::Reset)).is_err() {
            failed.push("style");
        }
        if execute!(self.output, LeaveAlternateScreen).is_err() {
            failed.push("alternate screen");
        }
        if execute!(self.output, Show).is_err() {
            failed.push("cursor");
        }
        if failed.is_empty() {
            Ok(())
        } else {
            Err(Error::new(
                "terminal",
                &format!("Terminal restoration failed: {}", failed.join(", ")),
            ))
        }
    }
}
impl<W: Write> Drop for TerminalGuard<W> {
    fn drop(&mut self) {
        if let Err(error) = self.restore() {
            eprintln!("{}", error.message);
        }
    }
}

/// Own the terminal and state on the calling task; communication never blocks input.
pub fn run(config: Config) -> Result<(), Error> {
    run_on(config, io::stdout())
}
fn run_on<W: Write>(config: Config, output: W) -> Result<(), Error> {
    run_shell_on(
        config,
        output,
        Shell::new(std::env::var_os("NO_COLOR").is_some()),
    )
}
fn run_shell_on<W: Write>(config: Config, output: W, mut shell: Shell) -> Result<(), Error> {
    let runtime = Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|_| Error::new("terminal", "Cannot initialize runtime"))?;
    let mut guard = TerminalGuard {
        output,
        original_termios: rustix::termios::tcgetattr(io::stdin())
            .map_err(|_| Error::new("terminal", "Cannot read terminal settings"))?,
        active: false,
    };
    let config = Arc::new(config);
    let mut pending = Vec::new();
    let result = catch_unwind(AssertUnwindSafe(|| runtime.block_on(async {
        // Register eagerly: raw-mode entry must never precede the SIGINT handler.
        let mut signal = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
            .map_err(|_| Error::new("terminal", "Cannot receive interrupt signals"))?;
        guard.enter()?;
        #[cfg(not(test))]
        let load_initial = true;
        #[cfg(test)]
        let load_initial = shell.input_check.is_none();
        if load_initial {
            shell.browse.start(&config);
        }
        let mut terminal = Terminal::new(CrosstermBackend::new(&mut guard.output))
            .map_err(|_| Error::new("terminal", "Cannot initialize terminal"))?;
        let mut interval = tokio::time::interval(Duration::from_millis(16));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                signal = signal.recv() => return Err(match signal {
                    Some(()) => Error::new("interrupted", "Operation interrupted"),
                    None => Error::new("terminal", "Cannot receive interrupt signals"),
                }),
                _ = interval.tick() => {
                    terminal.draw(|frame| shell.draw(frame))
                        .map_err(|_| Error::new("terminal", "Cannot draw terminal"))?;
                    let mut index = 0;
                    while index < pending.len() {
                        let (id, task): &mut (_, tokio::task::JoinHandle<browse::Response>) = &mut pending[index];
                        if !shell.browse.is_current(*id) {
                            task.abort();
                            let (_, task) = pending.swap_remove(index);
                            let _ = task.await;
                        } else if task.is_finished() {
                            let (_, task) = pending.swap_remove(index);
                            let response = task.await.map_err(|_| Error::new("terminal", "Read task failed"))?;
                            shell.browse.complete(response, config.settings.project.as_deref())?;
                            if std::mem::take(&mut shell.browse.focus_issue) { shell.set_focus(Focus::Issue); }
                        } else { index += 1; }
                    }
                    // Bound work per tick even during a stream of paste/resize events.
                    for _ in 0..32 {
                        if !event::poll(Duration::ZERO).map_err(|_| Error::new("terminal", "Cannot poll terminal input"))? { break; }
                        let event = event::read().map_err(|_| Error::new("terminal", "Cannot read terminal input"))?;
                        let size = terminal.size().map_err(|_| Error::new("terminal", "Cannot read terminal size"))?;
                        if let Some(code) = shell.event(event, Rect::new(0, 0, size.width, size.height)) {
                            return if code == 0 { Ok(()) } else { Err(Error::new("interrupted", "Operation interrupted")) };
                        }
                        if let Some(target) = shell.editor_request.take() {
                            drop(terminal);
                            guard.restore()?;
                            // Keep restoration armed during editor execution, including unwinding.
                            guard.active = true;
                            // No event polling while the editor owns the terminal.
                            let edited = match target {
                                EditorRequest::Settings(index) => {
                                    let path = if index == 0 { config::common_config_path()? } else {
                                        std::env::current_dir().map_err(|_| Error::new("editor", "Cannot locate cwd settings"))?.join(config::CWD_SETTINGS_PATH)
                                    };
                                    editor::edit_file(&config.editor, &path, async { signal.recv().await; }).await
                                }
                                #[cfg(test)]
                                EditorRequest::Body => editor::edit_body(&config.editor, shell.input_check.as_mut().unwrap().editor_body.as_mut().unwrap(), async { signal.recv().await; }).await,
                            };
                            if let Err(restore) = guard.restore() {
                                return match edited {
                                    Err(mut error) => {
                                        error.message.push_str(&format!("; {}", restore.message));
                                        Err(error)
                                    }
                                    Ok(()) => Err(restore),
                                };
                            }
                            if edited.as_ref().is_err_and(|error| error.kind == "interrupted") { return edited; }
                            shell.browse.error_notice = Some(safety::text(&match edited {
                                Ok(()) => match target {
                                    EditorRequest::Settings(_) => "Settings editor closed; restart to apply".into(),
                                    #[cfg(test)]
                                    EditorRequest::Body => "Body editor finished".into(),
                                },
                                Err(error) => error.message,
                            }, &[&config.api_key]));
                            guard.enter()?;
                            terminal = Terminal::new(CrosstermBackend::new(&mut guard.output)).map_err(|_| Error::new("terminal", "Cannot initialize terminal"))?;
                            let size = terminal.size().map_err(|_| Error::new("terminal", "Cannot read terminal size"))?;
                            // resize clears the viewport and resets the diff buffer even at the same size.
                            terminal.resize(Rect::new(0, 0, size.width, size.height)).map_err(|_| Error::new("terminal", "Cannot resize terminal"))?;
                            terminal.draw(|frame| shell.draw(frame)).map_err(|_| Error::new("terminal", "Cannot draw terminal"))?;
                            break;
                        }
                    }
                    shell.browse.tick();
                    for request in std::mem::take(&mut shell.browse.requests) {
                        let id = request.id();
                        if !shell.browse.is_current(id) { continue; }
                        let config = Arc::clone(&config);
                        pending.push((id, tokio::spawn(request.run(config))));
                    }
                }
            }
        }
    }))).unwrap_or_else(|_| Err(Error::new("terminal", "TUI stopped after an internal panic")));
    for (_, task) in pending {
        task.abort();
        let _ = runtime.block_on(task);
    }
    let restoration = guard.restore();
    // Started DNS lookups cannot be aborted; never wait for them after restoring the terminal.
    runtime.shutdown_background();
    let result = match (result, restoration) {
        (Ok(()), restored) => restored,
        (Err(mut error), Err(restore)) => {
            error.message.push_str(&format!("; {}", restore.message));
            Err(error)
        }
        (Err(error), Ok(())) => Err(error),
    };
    result.map_err(|mut error| {
        error.message = safety::text(&error.message, &[&config.api_key]);
        error
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::{io::Read, process::Command};

    #[test]
    fn search_dialog_is_centered_and_popup_overlays_without_moving_keyword() {
        use ratatui::backend::TestBackend;
        for (width, height, x, y, w, h) in [(100, 24, 14, 3, 72, 18), (40, 12, 1, 1, 38, 10)] {
            let mut shell = Shell::new(true);
            shell.browse.dialog = Some(search::Dialog::new(None, search::Conditions::default()));
            let area = Rect::new(0, 0, width, height);
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|frame| shell.draw(frame)).unwrap();
            let before = terminal.backend().buffer().clone();
            assert_eq!(before[(x, y)].symbol(), "┌");
            assert_eq!(before[(x + w - 1, y + h - 1)].symbol(), "┘");
            let text: String = before.content().iter().map(|cell| cell.symbol()).collect();
            assert!(text.replace(' ', "").contains("キーワード"), "{text}");
            let cursor = terminal.get_cursor_position().unwrap();
            shell.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE), area);
            terminal.draw(|frame| shell.draw(frame)).unwrap();
            let popup = terminal.backend().buffer();
            // Cross-project popup is 60x10 or 36x8, entirely inside the parent.
            let pw = 60.min(w - 2);
            let ph = 10.min(h - 2);
            let px = x + 1;
            let py = y + 1;
            assert_eq!(popup[(px, py)].symbol(), "┌");
            assert_eq!(popup[(px + pw - 1, py + ph - 1)].symbol(), "┘");
            shell.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), area);
            terminal.draw(|frame| shell.draw(frame)).unwrap();
            let after = terminal.backend().buffer();
            for column in cursor.x.saturating_sub(1)..x + w - 1 {
                assert_eq!(before[(column, cursor.y)], after[(column, cursor.y)]);
            }
            shell.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE), area);
            terminal.draw(|frame| shell.draw(frame)).unwrap();
            assert_eq!(terminal.get_cursor_position().unwrap(), cursor);
        }
    }

    #[test]
    fn search_dialog_shows_selected_values_beside_labels_and_empty_values_as_blank() {
        use ratatui::backend::TestBackend;
        for (width, height, x, y, field_width) in [
            (100, 24, 50, 4, 35),
            (40, 12, 2, 3, 36),
            (100, 12, 15, 3, 70),
        ] {
            let mut shell = Shell::new(true);
            shell.browse.dialog = Some(search::Dialog::new(None, search::Conditions::default()));
            let area = Rect::new(0, 0, width, height);
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|frame| shell.draw(frame)).unwrap();
            let cursor = terminal.get_cursor_position().unwrap();
            let row = |buffer: &ratatui::buffer::Buffer| -> String {
                let mut visible = String::new();
                let mut column = x;
                while column < x + field_width {
                    let symbol = buffer[(column, y)].symbol();
                    visible.push_str(symbol);
                    column += ratatui::text::Line::from(symbol).width().max(1) as u16;
                }
                visible
            };
            assert_eq!(row(terminal.backend().buffer()).replace(' ', ""), "担当者:");
            shell.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE), area);
            shell
                .browse
                .dialog
                .as_mut()
                .unwrap()
                .load_candidates(vec![search::Choice {
                    id: Some("7".into()),
                    label: format!("HEAD{}TAIL", "日e\u{301}👩‍💻".repeat(20)),
                }]);
            shell.key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE), area);
            for _ in 0..3 {
                shell.key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE), area);
            }
            terminal.draw(|frame| shell.draw(frame)).unwrap();
            let popup: String = terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|cell| cell.symbol())
                .collect();
            assert!(
                popup.contains("> [x] "),
                "horizontal scrolling must preserve the selection mark"
            );
            shell.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), area);
            terminal.draw(|frame| shell.draw(frame)).unwrap();
            assert!(
                row(terminal.backend().buffer())
                    .replace(' ', "")
                    .starts_with(">担当者:HEAD"),
                "{width}x{height}: {}",
                row(terminal.backend().buffer())
            );
            let mut saw_tail = false;
            for _ in 0..150 {
                shell.key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE), area);
                terminal.draw(|frame| shell.draw(frame)).unwrap();
                saw_tail |= row(terminal.backend().buffer()).contains("TAIL");
                assert_eq!(terminal.backend().buffer()[(x, y)].symbol(), ">");
            }
            assert!(
                saw_tail,
                "{width}x{height}: full selected value must remain reachable"
            );
            for _ in 0..150 {
                shell.key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE), area);
            }
            terminal.draw(|frame| shell.draw(frame)).unwrap();
            assert!(row(terminal.backend().buffer()).contains("HEAD"));
            shell.key(
                KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL),
                area,
            );
            terminal.draw(|frame| shell.draw(frame)).unwrap();
            assert_eq!(terminal.get_cursor_position().unwrap(), cursor);
        }
    }

    #[test]
    fn search_dialog_short_wide_terminal_keeps_keyword_border_and_help_separate() {
        use ratatui::backend::TestBackend;
        let mut shell = Shell::new(true);
        shell.browse.dialog = Some(search::Dialog::new(None, search::Conditions::default()));
        let mut terminal = Terminal::new(TestBackend::new(100, 12)).unwrap();
        terminal.draw(|frame| shell.draw(frame)).unwrap();
        let buffer = terminal.backend().buffer();
        assert_eq!(
            buffer[(15, 7)].symbol(),
            "└",
            "stack fields when height cannot fit side-by-side values and keyword frame"
        );
        assert_eq!(buffer[(84, 7)].symbol(), "┘");
        let help: String = (15..85).map(|x| buffer[(x, 8)].symbol()).collect();
        assert!(help.contains("Tab/ShiftTab"));
    }

    #[test]
    fn search_dialog_enter_requires_unmodified_press_and_popup_close_never_applies() {
        use ratatui::backend::TestBackend;
        let area = Rect::new(0, 0, 40, 12);
        let mut shell = Shell::new(true);
        shell.browse.dialog = Some(search::Dialog::new(
            None,
            search::Conditions {
                keyword: "draft".into(),
                ..Default::default()
            },
        ));
        let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
        for popup in [false, true] {
            if popup {
                shell.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE), area);
            }
            terminal.draw(|frame| shell.draw(frame)).unwrap();
            let before = terminal.backend().buffer().clone();
            for (modifiers, kind) in [
                (KeyModifiers::ALT, KeyEventKind::Press),
                (KeyModifiers::CONTROL, KeyEventKind::Press),
                (KeyModifiers::NONE, KeyEventKind::Repeat),
                (KeyModifiers::NONE, KeyEventKind::Release),
            ] {
                shell.key(
                    KeyEvent::new_with_kind(KeyCode::Enter, modifiers, kind),
                    area,
                );
                terminal.draw(|frame| shell.draw(frame)).unwrap();
                assert_eq!(&before, terminal.backend().buffer());
            }
        }
        shell.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), area);
        terminal.draw(|frame| shell.draw(frame)).unwrap();
        let closed: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(closed.contains("Cross-project Search") && !closed.contains("Applying..."));
        shell.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), area);
        terminal.draw(|frame| shell.draw(frame)).unwrap();
        let applying = terminal.backend().buffer().clone();
        assert!(
            applying
                .content()
                .iter()
                .map(|c| c.symbol())
                .collect::<String>()
                .contains("Applying...")
        );
        for (code, modifiers) in [
            (KeyCode::Char('x'), KeyModifiers::NONE),
            (KeyCode::Tab, KeyModifiers::NONE),
            (KeyCode::Char('u'), KeyModifiers::CONTROL),
            (KeyCode::Enter, KeyModifiers::NONE),
        ] {
            shell.key(KeyEvent::new(code, modifiers), area);
            terminal.draw(|frame| shell.draw(frame)).unwrap();
            assert_eq!(&applying, terminal.backend().buffer());
        }
        assert_eq!(
            shell.key(
                KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
                area
            ),
            Some(130)
        );
        shell.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), area);
        terminal.draw(|frame| shell.draw(frame)).unwrap();
        assert!(
            !terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|c| c.symbol())
                .collect::<String>()
                .contains("Cross-project Search")
        );
    }

    #[test]
    fn search_dialog_candidate_horizontal_scroll_preserves_graphemes_checks_and_full_safe_values() {
        use ratatui::backend::TestBackend;
        let area = Rect::new(0, 0, 40, 12);
        let name = format!("{}TAIL", "日e\u{301}👩‍💻".repeat(20));
        let mut shell = Shell::new(true);
        shell.browse.secret = "test-secret".into();
        shell.browse.dialog = Some(search::Dialog::new(None, search::Conditions::default()));
        shell.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE), area);
        shell.browse.dialog.as_mut().unwrap().load_candidates(vec![
            search::Choice {
                id: Some("7".into()),
                label: name,
            },
            search::Choice {
                id: Some("8".into()),
                label: format!(
                    "{}test-secret\u{1b}]52;c;secret\u{7}\n\t SAFE-END",
                    "x".repeat(100)
                ),
            },
        ]);
        let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
        shell.key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE), area);
        for expected in ["日", "e\u{301}", "👩‍💻"] {
            terminal.draw(|frame| shell.draw(frame)).unwrap();
            let buffer = terminal.backend().buffer();
            assert_eq!(buffer[(9, 3)].symbol(), expected);
            assert_eq!(buffer[(3, 3)].symbol(), ">");
            assert_eq!(buffer[(6, 3)].symbol(), "x");
            shell.key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE), area);
        }
        let mut saw_tail = false;
        for _ in 0..100 {
            terminal.draw(|frame| shell.draw(frame)).unwrap();
            let visible: String = terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|c| c.symbol())
                .collect();
            saw_tail |= visible.contains("TAIL");
            assert!(!visible.contains("test-secret") && !visible.contains('\u{1b}'));
            shell.key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE), area);
        }
        assert!(saw_tail);
        shell.key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), area);
        terminal.draw(|frame| shell.draw(frame)).unwrap();
        assert_eq!(
            terminal.backend().buffer()[(9, 4)].symbol(),
            "x",
            "changing candidate resets horizontal position"
        );
        let mut saw_safe_end = false;
        let mut saw_line_escapes = false;
        for _ in 0..150 {
            terminal.draw(|frame| shell.draw(frame)).unwrap();
            let visible: String = terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|c| c.symbol())
                .collect();
            saw_safe_end |= visible.contains("SAFE-END");
            saw_line_escapes |= visible.contains("\\n\\t");
            assert!(!visible.contains("test-secret") && !visible.contains('\u{1b}'));
            shell.key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE), area);
        }
        assert!(saw_safe_end && saw_line_escapes);
        shell.key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE), area);
        shell.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), area);
        // Every selected value remains reachable on the closed field's single line too.
        let mut saw_tail = false;
        let mut saw_safe_end = false;
        for _ in 0..250 {
            terminal.draw(|frame| shell.draw(frame)).unwrap();
            let visible: String = terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|c| c.symbol())
                .collect();
            saw_tail |= visible.contains("TAIL");
            saw_safe_end |= visible.contains("SAFE-END");
            assert!(!visible.contains("test-secret") && !visible.contains('\u{1b}'));
            shell.key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE), area);
        }
        assert!(saw_tail && saw_safe_end);
    }

    pub(super) struct InputCheck {
        pub(super) field: SingleLineInput,
        pub(super) focused: bool,
        pub(super) editor_body: Option<String>,
    }
    impl InputCheck {
        fn new() -> Self {
            Self {
                field: SingleLineInput::new(String::new()),
                focused: true,
                editor_body: None,
            }
        }
        pub(super) fn key(&mut self, key: KeyEvent, area: Rect) -> Option<u8> {
            if area.width < 40 || area.height < 12 {
                return (key.code == KeyCode::Char('q') && key.modifiers.is_empty()).then_some(0);
            }
            match key.code {
                KeyCode::Tab | KeyCode::BackTab
                    if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
                {
                    self.focused = !self.focused;
                }
                KeyCode::Esc if key.modifiers.is_empty() => self.focused = false,
                _ if self.focused => self.field.event(&Event::Key(key), area),
                KeyCode::Char('q') if key.modifiers.is_empty() => return Some(0),
                _ => {}
            }
            None
        }
        pub(super) fn draw(&self, frame: &mut Frame<'_>) {
            frame.render_widget(Paragraph::new(if self.editor_body.is_some() { "Body editor check (no network)\nTab: other field | Esc: leave field\ne: editor (outside field) q: quit Ctrl+C" } else { "Input check (no network)\nTab/Shift+Tab: focus | Esc: leave input\nOutside input: q: quit | Ctrl+C: interrupt" }), Rect::new(0, 0, frame.area().width, 3));
            if let Some(body) = &self.editor_body {
                frame.render_widget(
                    Paragraph::new(safety::text(body, &["test-secret"])).wrap(Wrap { trim: false }),
                    Rect::new(
                        1,
                        6,
                        frame.area().width.saturating_sub(2),
                        frame.area().height.saturating_sub(6),
                    ),
                );
            }
            let field = Rect::new(1, 4, frame.area().width.saturating_sub(2), 1);
            self.field
                .draw(frame, field, self.focused, &["test-secret"]);
            frame.render_widget(
                Paragraph::new(if self.focused {
                    "> Input"
                } else {
                    "Input (unfocused)"
                }),
                Rect::new(0, 3, frame.area().width, 1),
            );
        }
    }

    /// Same loop/guard/editor operation as settings; no product-only demo or flags.
    #[test]
    #[ignore = "manual editor acceptance; see docs/testing/manual-acceptance.md"]
    fn manual_body_editor_terminal() {
        use std::io::IsTerminal;
        assert!(io::stdin().is_terminal() && io::stdout().is_terminal());
        let mut shell = Shell::new(std::env::var_os("NO_COLOR").is_some());
        let mut input = InputCheck::new();
        input.editor_body = Some("  原文 test-secret\r\n".into());
        input.field = SingleLineInput::new("other-value e\u{301} 👩‍💻".into());
        input.focused = false;
        shell.input_check = Some(input);
        shell.browse.secret = "test-secret".into();
        let config = Config {
            origin: "https://localhost:9".parse().unwrap(),
            api_key: "test-secret".into(),
            settings: Default::default(),
            editor: std::env::var("BL_TEST_EDITOR_ARGV")
                .map(|s| serde_json::from_str(&s).unwrap())
                .unwrap_or_else(|_| vec!["vim".into()]),
        };
        let output = FaultOutput {
            output: io::stdout(),
            seen: vec![],
            fault: std::env::var("SHELL_TEST_FAULT").unwrap_or_default(),
            triggered: false,
        };
        if let Err(error) = run_shell_on(config, output, shell) {
            eprintln!("{}", error.message);
            std::process::exit(error.exit_code().into());
        }
    }

    #[test]
    fn body_editor_roundtrip_and_failures_use_shared_terminal_handoff() {
        for case in [
            "body",
            "nonzero",
            "signal",
            "utf8",
            "unlink",
            "interrupt",
            "spawn",
            "raw-body",
            "raw-nonzero",
            "raw-signal",
            "raw-interrupt",
            "raw-interrupt-restore-error",
            "raw-nonzero-restore-error",
        ] {
            let (mode, fault) = case
                .strip_suffix("-restore-error")
                .map_or((case, ""), |mode| (mode, "editor-restore-error"));
            let outcome = mode.strip_prefix("raw-").unwrap_or(mode);
            let dir = tempfile::tempdir().unwrap();
            let steps = dir.path().join("steps.json");
            let mut actions = vec![serde_json::json!({"wait":"Body editor check", "keys":"e"})];
            if outcome == "interrupt" {
                actions.push(serde_json::json!({"wait":"EDITOR READY", "signal":"SIGINT"}));
            } else {
                if mode != "spawn" {
                    actions.push(serde_json::json!({"wait":"EDITOR READY", "resize":[12,40], "keys":"qjk\n"}));
                }
                let notice = match outcome {
                    "body" => "Body editor finished",
                    "nonzero" | "signal" => "Editor did not exit successfully",
                    "spawn" => "Cannot run editor",
                    _ => "Cannot read editor file as UTF-8",
                };
                if fault.is_empty() {
                    actions.push(serde_json::json!({"wait":notice, "keys":"\u{1b}"}));
                    actions.push(serde_json::json!({"wait":"Input (unfocused)", "keys":"q"}));
                }
            }
            std::fs::write(&steps, serde_json::to_string(&actions).unwrap()).unwrap();
            let record = dir.path().join("editor-record");
            let argv = if mode == "spawn" {
                serde_json::json!(["/no/such/editor"])
            } else {
                serde_json::json!([
                    "/usr/bin/python3",
                    concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/editor.py"),
                    record,
                    mode
                ])
            };
            let output = Command::new("python3")
                .arg(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/tty_shell.py"
                ))
                .arg("scripted")
                .arg(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "tui::tests::manual_body_editor_terminal",
                    "--ignored",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env("SHELL_STEPS_FILE", &steps)
                .env("TMPDIR", dir.path())
                .env("SHELL_REQUESTS_FILE", dir.path().join("requests"))
                .env("BL_TEST_EDITOR_ARGV", argv.to_string())
                .env("SHELL_TEST_FAULT", fault)
                .output()
                .unwrap();
            assert!(output.status.success(), "{mode}: {output:?}");
            let result: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(
                result["code"],
                if outcome == "interrupt" {
                    130
                } else if !fault.is_empty() {
                    1
                } else {
                    0
                },
                "{mode}: {result}"
            );
            assert_eq!(result["restored"], true, "{mode}: {result}");
            assert_eq!(result["termios_diff"], serde_json::json!([]));
            if mode != "spawn" {
                let record: Value =
                    serde_json::from_str(&std::fs::read_to_string(record).unwrap()).unwrap();
                assert_eq!(record["mode"], 384);
                assert_eq!(record["original"], "  原文 test-secret\r\n");
                assert_eq!(record["cooked"], true);
                assert_eq!(record["echo"], true);
                if mode.starts_with("raw-") {
                    assert_eq!(record["raw_noecho"], true);
                }
                assert!(!std::path::Path::new(record["path"].as_str().unwrap()).exists());
                if outcome == "interrupt" {
                    assert!(
                        !Command::new("/bin/kill")
                            .args(["-0", &record["pid"].to_string()])
                            .output()
                            .unwrap()
                            .status
                            .success()
                    );
                } else {
                    assert_eq!(record["input"], "qjk\n");
                }
            }
            for entry in std::fs::read_dir(dir.path()).unwrap() {
                let name = entry.unwrap().file_name();
                assert!(
                    name == "steps.json" || name == "editor-record",
                    "temporary editor data remains: {name:?}"
                );
            }
            if !fault.is_empty() {
                let stderr = result["stderr"].as_str().unwrap();
                assert!(
                    stderr.contains("Terminal restoration failed: alternate screen"),
                    "{case}: {result}"
                );
                assert!(
                    stderr.contains(if outcome == "interrupt" {
                        "Operation interrupted"
                    } else {
                        "Editor did not exit successfully"
                    }),
                    "{case}: {result}"
                );
                continue;
            }
            let screen = result["screen"].as_str().unwrap();
            if outcome == "body" {
                assert!(screen.contains("編") && screen.contains("後"), "{result}");
            } else if outcome != "interrupt" {
                assert!(
                    screen.contains("原") && screen.contains("[REDACTED]"),
                    "{mode}: {result}"
                );
            }
            if outcome != "interrupt" {
                assert!(
                    screen.contains("other-value"),
                    "other field lost: {mode}: {result}"
                );
                assert!(screen.contains("Input (unfocused)"));
                assert!(!result["stdout"].as_str().unwrap().contains("test-secret"));
            }
        }
    }

    /// Run directly in a real terminal; never enabled by the normal test suite.
    #[test]
    #[ignore = "manual real-terminal/IME acceptance; see docs/testing/manual-acceptance.md"]
    fn manual_single_line_input_terminal() {
        use std::io::IsTerminal;
        assert!(
            io::stdin().is_terminal() && io::stdout().is_terminal(),
            "stdin and stdout must both be TTYs"
        );
        let mut shell = Shell::new(std::env::var_os("NO_COLOR").is_some());
        shell.input_check = Some(InputCheck::new());
        let config = Config {
            origin: "https://localhost:9".parse().unwrap(),
            api_key: "test-secret".into(),
            settings: Default::default(),
            editor: vec!["vim".into()],
        };
        if let Err(error) = run_shell_on(config, io::stdout(), shell) {
            eprintln!("{}", error.message);
            std::process::exit(error.exit_code().into());
        }
    }

    #[test]
    fn input_shell_routes_editing_keys_before_global_keys_and_retains_values_on_focus_and_resize() {
        use ratatui::backend::TestBackend;
        let mut shell = Shell::new(true);
        shell.input_check = Some(InputCheck::new());
        let area = Rect::new(0, 0, 40, 12);
        for c in "qhljk".chars() {
            assert_eq!(
                shell.event(
                    Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)),
                    area
                ),
                None
            );
        }
        assert_eq!(shell.input_check.as_ref().unwrap().field.value(), "qhljk");
        for code in [KeyCode::Left, KeyCode::Backspace] {
            shell.event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)), area);
        }
        assert_eq!(shell.input_check.as_ref().unwrap().field.value(), "qhlk");
        for code in [KeyCode::Tab, KeyCode::BackTab, KeyCode::Esc] {
            shell.event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)), area);
        }
        assert_eq!(shell.input_check.as_ref().unwrap().field.value(), "qhlk");
        shell.event(Event::Paste("ignored".into()), area);
        assert_eq!(shell.input_check.as_ref().unwrap().field.value(), "qhlk");
        shell.event(
            Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
            area,
        );
        let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
        terminal.draw(|frame| shell.draw(frame)).unwrap();
        assert!(terminal.backend().cursor_visible());
        let tiny = Rect::new(0, 0, 39, 12);
        for code in [KeyCode::Backspace, KeyCode::Esc, KeyCode::Tab] {
            shell.event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)), tiny);
        }
        shell.event(Event::Paste("ignored".into()), tiny);
        assert_eq!(shell.input_check.as_ref().unwrap().field.value(), "qhlk");
        assert!(shell.input_check.as_ref().unwrap().focused);
        assert_eq!(
            shell.event(
                Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
                area
            ),
            Some(130)
        );
        assert_eq!(
            shell.event(
                Event::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)),
                tiny
            ),
            Some(0)
        );
        shell.event(
            Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            area,
        );
        assert_eq!(
            shell.event(
                Event::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)),
                area
            ),
            Some(0)
        );
    }

    #[test]
    fn input_shell_pty_normalizes_and_masks_paste_and_restores_the_terminal() {
        for (mode, code) in [("input-quit", 0), ("input-interrupt", 130)] {
            let output = Command::new("python3")
                .arg(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/tty_shell.py"
                ))
                .arg(mode)
                .arg(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "tui::tests::manual_single_line_input_terminal",
                    "--ignored",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .output()
                .unwrap();
            assert!(output.status.success(), "{output:?}");
            let result: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(result["code"], code, "{result}");
            assert_eq!(result["restored"], true, "{result}");
            let screen = result["stdout"].as_str().unwrap();
            assert!(screen.contains("[REDACTED]"), "{result}");
            assert!(screen.contains("\x1b[?2004h") && screen.contains("\x1b[?2004l"));
            assert!(!result.to_string().contains("test-secret"));
            assert!(!screen.contains("\x1b]52"));
        }
    }

    #[test]
    fn detail_document_scroll_keys_wrap_resize_and_right_focus_keep_the_selected_document() {
        use ratatui::backend::TestBackend;
        fn screen(shell: &Shell, w: u16, h: u16) -> String {
            let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
            terminal.draw(|frame| shell.draw(frame)).unwrap();
            terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect()
        }
        let mut shell = Shell::new(true);
        shell.focus = Focus::Detail;
        shell.browse.detail.replace(
            (1..=30)
                .map(|n| {
                    format!(
                        "Line {n:02} Japanese 日本語 e\u{301} 👩‍💻 wrapping across narrow panels\n"
                    )
                })
                .collect(),
        );
        let area = Rect::new(0, 0, 40, 12);
        let press = |shell: &mut Shell, code| {
            shell.key(KeyEvent::new(code, KeyModifiers::NONE), area);
        };
        assert!(screen(&shell, 40, 12).contains("Line 01"));
        for down in [KeyCode::Down, KeyCode::Char('j')] {
            press(&mut shell, down);
        }
        assert!(!screen(&shell, 40, 12).contains("Line 01"));
        for up in [KeyCode::Up, KeyCode::Char('k')] {
            press(&mut shell, up);
        }
        assert!(screen(&shell, 40, 12).contains("Line 01"));
        for (down, up) in [
            (KeyCode::PageDown, KeyCode::PageUp),
            (KeyCode::Char(' '), KeyCode::Char('b')),
        ] {
            press(&mut shell, down);
            assert!(!screen(&shell, 40, 12).contains("Line 01"));
            press(&mut shell, up);
            assert!(screen(&shell, 40, 12).contains("Line 01"));
        }
        for (end, home) in [
            (KeyCode::End, KeyCode::Home),
            (KeyCode::Char('G'), KeyCode::Char('g')),
        ] {
            press(&mut shell, end);
            assert!(screen(&shell, 40, 12).contains("Line 30"));
            press(&mut shell, home);
            assert!(screen(&shell, 40, 12).contains("Line 01"));
        }
        press(&mut shell, KeyCode::End);
        let bottom = screen(&shell, 40, 12);
        for toggle in [
            KeyCode::Char('h'),
            KeyCode::Char('l'),
            KeyCode::Tab,
            KeyCode::BackTab,
        ] {
            press(&mut shell, toggle);
            assert!(screen(&shell, 40, 12).contains("> Comment"));
            press(&mut shell, toggle);
            assert_eq!(screen(&shell, 40, 12), bottom);
        }
        // A taller document viewport clamps the old end offset and still shows its tail.
        assert!(screen(&shell, 100, 60).contains("Line 30"));
        // Width changes reflow the same Unicode document, not a prewrapped cache.
        let wide = screen(&shell, 160, 60);
        assert!(
            wide.contains("Line 01 Japanese") && wide.contains("wrapping across narrow panels")
        );
        assert!(wide.contains("Line 30"));
        press(&mut shell, KeyCode::Esc);
        assert!(screen(&shell, 40, 12).contains("> Issue List"));
    }

    #[test]
    fn comment_document_j_k_wrap_resize_clamp_and_focus_preserve_text_without_using_g_g_as_body_ends()
     {
        use ratatui::backend::TestBackend;
        fn screen(shell: &Shell, w: u16, h: u16) -> String {
            let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
            terminal.draw(|frame| shell.draw(frame)).unwrap();
            terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect()
        }
        let mut shell = Shell::new(true);
        shell.set_focus(Focus::Comment);
        shell.browse.comment.document.replace(
            (1..=30)
                .map(|n| format!("Line {n:02} 日本語 e\u{301} 👩‍💻 wraps across narrow panels\n"))
                .collect(),
        );
        let area = Rect::new(0, 0, 40, 12);
        let press = |shell: &mut Shell, code| {
            shell.key(KeyEvent::new(code, KeyModifiers::NONE), area);
        };
        let first = screen(&shell, 40, 12);
        assert!(first.contains("Line 01"));
        press(&mut shell, KeyCode::Char('j'));
        assert_ne!(screen(&shell, 40, 12), first);
        press(&mut shell, KeyCode::Char('k'));
        assert_eq!(screen(&shell, 40, 12), first);
        for _ in 0..100 {
            press(&mut shell, KeyCode::Char('j'));
        }
        let bottom = screen(&shell, 40, 12);
        assert!(bottom.contains("Line 30"));
        for code in [KeyCode::Char('g'), KeyCode::Char('G')] {
            press(&mut shell, code);
            assert_eq!(
                screen(&shell, 40, 12),
                bottom,
                "comment endpoints never scroll the body"
            );
        }
        for code in [
            KeyCode::Char('h'),
            KeyCode::Char('l'),
            KeyCode::Tab,
            KeyCode::BackTab,
        ] {
            press(&mut shell, code);
            press(&mut shell, code);
            assert_eq!(screen(&shell, 40, 12), bottom);
        }
        assert!(screen(&shell, 100, 60).contains("Line 30"));
        let wide = screen(&shell, 200, 200);
        assert!(
            wide.contains("Line 01")
                && wide.contains("wraps across narrow panels")
                && wide.contains("Line 30"),
            "first={} wrap={} last={}",
            wide.contains("Line 01"),
            wide.contains("wraps across narrow panels"),
            wide.contains("Line 30")
        );
        assert!(shell.browse.requests.is_empty());
    }

    #[test]
    fn error_popup_wraps_inside_both_layouts_and_uses_bold_without_color_under_no_color() {
        use ratatui::backend::TestBackend;
        for no_color in [false, true] {
            let mut shell = Shell::new(no_color);
            shell.browse.error_notice = Some("Failed: 再取得に失敗しました。日本語の長い通知も折り返して表示します。\nRetry later".into());
            for (width, height) in [(40, 12), (100, 24)] {
                let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
                terminal.draw(|frame| shell.draw(frame)).unwrap();
                let buffer = terminal.backend().buffer();
                let text: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
                assert!(text.contains("Retry later"), "{text}");
                assert!(!text.contains("ERROR") && !text.contains("OK"));
                let left = if width == 40 { 2 } else { 12 };
                let top = (1..height - 1)
                    .find(|&row| buffer[(left, row)].symbol() == "┌")
                    .unwrap();
                let bottom = (top + 1..height - 1)
                    .find(|&row| buffer[(left, row)].symbol() == "└")
                    .unwrap();
                assert!((i32::from(top) - i32::from(height - bottom - 1)).abs() <= 1);
                assert!(bottom - top > 2, "long message must wrap");
                for row in top..=bottom {
                    let cell = &buffer[(left, row)];
                    assert!(cell.modifier.contains(Modifier::BOLD));
                    assert_eq!(cell.fg, if no_color { Color::Reset } else { Color::Red });
                }
                if no_color {
                    assert!(
                        buffer
                            .content
                            .iter()
                            .all(|cell| { cell.fg == Color::Reset && cell.bg == Color::Reset })
                    );
                }
            }
        }
    }

    #[test]
    fn error_popup_dismissal_consumes_only_unmodified_enter_escape_space_and_preserves_document() {
        use ratatui::{backend::TestBackend, buffer::Buffer};
        fn draw(shell: &Shell, area: Rect) -> Buffer {
            let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
            terminal.draw(|frame| shell.draw(frame)).unwrap();
            terminal.backend().buffer().clone()
        }
        let area = Rect::new(0, 0, 40, 12);
        let mut shell = Shell::new(true);
        shell.focus = Focus::Detail;
        shell
            .browse
            .detail
            .replace((1..=30).map(|n| format!("Line {n:02}\n")).collect());
        shell.key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE), area);
        let retained = draw(&shell, area);
        for code in [KeyCode::Enter, KeyCode::Esc, KeyCode::Char(' ')] {
            shell.browse.error_notice = Some("Failed: retained document".into());
            let popup = draw(&shell, area);
            for modifiers in [KeyModifiers::CONTROL, KeyModifiers::ALT] {
                shell.key(KeyEvent::new(code, modifiers), area);
                assert_eq!(draw(&shell, area), popup);
            }
            shell.key(
                KeyEvent::new_with_kind(code, KeyModifiers::NONE, KeyEventKind::Release),
                area,
            );
            assert_eq!(draw(&shell, area), popup);
            let tiny = Rect::new(0, 0, 39, 12);
            shell.event(Event::Resize(39, 12), tiny);
            shell.key(KeyEvent::new(code, KeyModifiers::NONE), tiny);
            draw(&shell, tiny);
            shell.event(Event::Resize(40, 12), area);
            assert_eq!(draw(&shell, area), popup);
            assert_eq!(
                shell.key(
                    KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
                    area
                ),
                Some(130)
            );
            assert_eq!(draw(&shell, area), popup);
            shell.key(
                KeyEvent::new_with_kind(code, KeyModifiers::NONE, KeyEventKind::Repeat),
                area,
            );
            assert_eq!(
                draw(&shell, area),
                retained,
                "dismissal must not navigate or scroll"
            );
        }
        shell.key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE), area);
        assert_ne!(
            draw(&shell, area),
            retained,
            "Space without a popup still scrolls"
        );
        shell.browse.error_notice = Some("Failed: retained document".into());
        shell.key(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE), area);
        shell.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE), area);
        assert!(shell.browse.error_notice.is_none());
        shell.browse.error_notice = Some("Failed: retained document".into());
        shell.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), area);
        let text: String = draw(&shell, area)
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(text.contains("> Comment"), "other keys still change focus");
        shell.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), area);
        let text: String = draw(&shell, area)
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(
            text.contains("> Issue List"),
            "second Esc still returns to Issue List"
        );
        shell.browse.error_notice = Some("Failed: retained document".into());
        shell.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT), area);
        assert!(shell.browse.error_notice.is_none());
        let text: String = draw(&shell, area)
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(
            text.contains("> Issue Detail"),
            "modified Enter retains its normal action"
        );
    }

    #[test]
    fn error_popup_clears_before_normal_scroll_focus_and_browser_keys_execute() {
        use ratatui::backend::TestBackend;
        let area = Rect::new(0, 0, 40, 12);
        let mut shell = Shell::new(true);
        shell.focus = Focus::Detail;
        shell
            .browse
            .detail
            .replace((1..=30).map(|n| format!("Line {n:02}\n")).collect());
        for (code, expected) in [
            (KeyCode::Char('j'), "Line 02"),
            (KeyCode::Char('k'), "Line 01"),
            (KeyCode::Char('h'), "> Comment"),
            (KeyCode::Char('l'), "> Issue Detail"),
        ] {
            shell.browse.error_notice = Some("Failed: old notification".into());
            for ignored in [
                KeyEvent::new(code, KeyModifiers::ALT),
                KeyEvent::new(code, KeyModifiers::CONTROL),
                KeyEvent::new_with_kind(code, KeyModifiers::NONE, KeyEventKind::Release),
            ] {
                shell.key(ignored, area);
                assert!(shell.browse.error_notice.is_some());
            }
            shell.key(KeyEvent::new(code, KeyModifiers::NONE), area);
            let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
            terminal.draw(|frame| shell.draw(frame)).unwrap();
            let text: String = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect();
            assert!(!text.contains("Failed:"), "{code:?}: {text}");
            assert!(text.contains(expected), "{code:?}: {text}");
            if code == KeyCode::Char('j') {
                assert!(!text.contains("Line 01"), "j must scroll: {text}");
            }
        }
        shell.focus = Focus::Issue;
        shell.browse.error_notice = Some("Failed: old notification".into());
        shell.key(
            KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL),
            area,
        );
        let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
        terminal.draw(|frame| shell.draw(frame)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(!text.contains("Failed:"), "{text}");
        assert!(text.contains("> Issue List"), "{text}");
    }

    #[test]
    fn region_keys_dismiss_notices_only_for_valid_main_screen_presses() {
        use ratatui::backend::TestBackend;
        fn screen(shell: &Shell) -> String {
            let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
            terminal.draw(|frame| shell.draw(frame)).unwrap();
            terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect()
        }
        let area = Rect::new(0, 0, 40, 12);
        let mut shell = Shell::new(true);
        for (code, expected) in [
            ('H', "> Project List"),
            ('L', "> Issue Detail"),
            ('L', "> Issue Detail"),
            ('H', "> Project List"),
        ] {
            shell.browse.error_notice = Some("Failed: retain this notice".into());
            let before = screen(&shell);
            for kind in [
                KeyEventKind::Press,
                KeyEventKind::Repeat,
                KeyEventKind::Release,
            ] {
                for modifiers in [
                    KeyModifiers::NONE,
                    KeyModifiers::SHIFT,
                    KeyModifiers::CONTROL,
                    KeyModifiers::ALT,
                    KeyModifiers::CONTROL | KeyModifiers::SHIFT,
                    KeyModifiers::ALT | KeyModifiers::SHIFT,
                ] {
                    if kind == KeyEventKind::Press
                        && (modifiers.is_empty() || modifiers == KeyModifiers::SHIFT)
                    {
                        continue;
                    }
                    shell.key(
                        KeyEvent::new_with_kind(KeyCode::Char(code), modifiers, kind),
                        area,
                    );
                    assert_eq!(screen(&shell), before);
                }
            }
            shell.key(
                KeyEvent::new(KeyCode::Char(code), KeyModifiers::NONE),
                Rect::new(0, 0, 39, 12),
            );
            assert_eq!(screen(&shell), before);
            shell.key(
                KeyEvent::new(KeyCode::Char(code), KeyModifiers::SHIFT),
                area,
            );
            let after = screen(&shell);
            assert!(!after.contains("Failed:"), "{after}");
            assert!(after.contains(expected), "{after}");
        }
    }

    #[test]
    fn detail_document_scroll_upper_bound_does_not_overflow() {
        use ratatui::backend::TestBackend;
        let mut shell = Shell::new(true);
        shell.focus = Focus::Detail;
        shell.browse.detail.replace("Row\n".repeat(65_550).into());
        let area = Rect::new(0, 0, 40, 12);
        for code in [
            KeyCode::End,
            KeyCode::Down,
            KeyCode::Char(' '),
            KeyCode::PageDown,
        ] {
            shell.key(KeyEvent::new(code, KeyModifiers::NONE), area);
        }
        let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
        terminal.draw(|frame| shell.draw(frame)).unwrap();
        assert!(
            terminal
                .backend()
                .buffer()
                .content
                .iter()
                .any(|cell| cell.symbol() == "R")
        );
        shell.key(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE), area);
        terminal.draw(|frame| shell.draw(frame)).unwrap();
    }

    // Inject faults at the terminal Write boundary, never through a product debug flag.
    struct FaultOutput {
        output: io::Stdout,
        seen: Vec<u8>,
        fault: String,
        triggered: bool,
    }
    impl Write for FaultOutput {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            let fault = self.fault.strip_prefix("blocking-").unwrap_or(&self.fault);
            if self.fault.starts_with("blocking-") && bytes == b"\x1b[?25l" {
                let (started, wait) = std::sync::mpsc::channel();
                tokio::task::spawn_blocking(move || {
                    started.send(()).unwrap();
                    // Model a started DNS lookup that cannot be aborted.
                    loop {
                        std::thread::park();
                    }
                });
                wait.recv().unwrap();
            }
            if !self.triggered
                && matches!(fault, "draw-error" | "panic" | "restore-error")
                && self.seen.windows(12).any(|w| w == b"Project List")
            {
                self.triggered = true;
                if fault == "draw-error" {
                    return Err(io::Error::other("test-secret\x1b[31m"));
                }
                panic!("test-secret\x1b]52;c;secret\x07");
            }
            self.seen.extend_from_slice(bytes);
            let written = self.output.write(bytes)?;
            if self.fault == "early-signal" && bytes == b"\x1b[?25l" {
                self.output.flush()?;
                // Hold entry before the first frame until the PTY driver sends SIGINT.
                io::stdin().read_exact(&mut [0])?;
            }
            if self.fault == "editor-restore-error" && bytes == b"\x1b[?1049l" {
                if self.triggered {
                    return Err(io::Error::other("test-secret"));
                }
                self.triggered = true;
            }
            if self.fault == "restore-error" && bytes == b"\x1b[?1049l" {
                return Err(io::Error::other("test-secret"));
            }
            Ok(written)
        }
        fn flush(&mut self) -> io::Result<()> {
            self.output.flush()
        }
    }

    #[test]
    fn terminal_fault_child() {
        let Ok(fault) = std::env::var("SHELL_TEST_FAULT") else {
            return;
        };
        std::panic::set_hook(Box::new(|_| {}));
        // Keep startup pending without relying on external DNS or network timing.
        let listener = (fault.starts_with("blocking-") || fault == "early-signal")
            .then(|| std::net::TcpListener::bind("127.0.0.1:0").unwrap());
        let config = Config {
            origin: listener.as_ref().map_or_else(
                || "https://localhost:9".parse().unwrap(),
                |listener| {
                    format!("https://{}", listener.local_addr().unwrap())
                        .parse()
                        .unwrap()
                },
            ),
            api_key: "test-secret".into(),
            settings: Default::default(),
            editor: vec!["vim".into()],
        };
        let result = run_on(
            config,
            FaultOutput {
                output: io::stdout(),
                seen: vec![],
                fault: fault.clone(),
                triggered: false,
            },
        );
        if fault == "blocking-quit" {
            assert!(result.is_ok(), "{result:?}");
            return;
        }
        let error = result.unwrap_err();
        if matches!(
            fault.as_str(),
            "early-signal" | "blocking-interrupt" | "blocking-signal"
        ) {
            assert_eq!(error.exit_code(), 130);
            eprintln!("{}", error.message);
            std::process::exit(130);
        }
        assert_eq!(error.exit_code(), 1);
        assert!(
            error.message.contains(if fault.ends_with("draw-error") {
                "Cannot draw terminal"
            } else {
                "internal panic"
            }),
            "{}",
            error.message
        );
        if fault == "restore-error" {
            assert!(
                error
                    .message
                    .contains("Terminal restoration failed: alternate screen")
            );
        }
        eprintln!("{}", error.message);
    }

    #[test]
    fn shell_exits_promptly_with_started_blocking_work_on_quit_interrupt_error_and_panic() {
        for (mode, fault, code) in [
            ("quit", "blocking-quit", 0),
            ("interrupt", "blocking-interrupt", 130),
            ("signal", "blocking-signal", 130),
            ("failure", "blocking-draw-error", 0),
            ("failure", "blocking-panic", 0),
        ] {
            let output = Command::new("python3")
                .arg(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/tty_shell.py"
                ))
                .arg(mode)
                .arg(std::env::current_exe().unwrap())
                .args(["--exact", "tui::tests::terminal_fault_child", "--nocapture"])
                .env("SHELL_TEST_FAULT", fault)
                .output()
                .unwrap();
            assert!(output.status.success(), "{fault}: {output:?}");
            let result: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(result["code"], code, "{fault}: {result}");
            assert_eq!(result["restored"], true, "{fault}: {result}");
            assert!(
                result["elapsed"].as_f64().unwrap() < 5.0,
                "{fault}: {result}"
            );
            assert!(
                result["stdout"]
                    .as_str()
                    .unwrap()
                    .contains("\x1b[0m\x1b[?1049l\x1b[?25h"),
                "{fault}: {result}"
            );
            assert!(
                !result.to_string().contains("test-secret"),
                "{fault}: {result}"
            );
        }
    }

    #[test]
    fn shell_restores_when_sigint_arrives_during_entry_before_the_first_frame() {
        let output = Command::new("python3")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/tty_shell.py"
            ))
            .arg("early-signal")
            .arg(std::env::current_exe().unwrap())
            .args(["--exact", "tui::tests::terminal_fault_child", "--nocapture"])
            .env("SHELL_TEST_FAULT", "early-signal")
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let result: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["code"], 130, "{result}");
        assert_eq!(result["restored"], true, "{result}");
        assert!(
            result["stdout"]
                .as_str()
                .unwrap()
                .contains("\x1b[0m\x1b[?1049l\x1b[?25h"),
            "{result}"
        );
        assert!(
            result["stderr"].as_str().unwrap().contains("interrupted"),
            "{result}"
        );
    }

    #[test]
    fn shell_restores_after_draw_errors_panics_and_reports_restore_failures() {
        for fault in ["draw-error", "panic", "restore-error"] {
            let output = Command::new("python3")
                .arg(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/tty_shell.py"
                ))
                .arg("failure")
                .arg(std::env::current_exe().unwrap())
                .args(["--exact", "tui::tests::terminal_fault_child", "--nocapture"])
                .env("SHELL_TEST_FAULT", fault)
                .output()
                .unwrap();
            assert!(output.status.success(), "{output:?}");
            let result: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(result["code"], 0, "{fault}: {result}");
            assert_eq!(result["restored"], true, "{fault}: {result}");
            let screen = result["stdout"].as_str().unwrap();
            assert!(
                screen.contains("\x1b[?2004h"),
                "paste not enabled: {result}"
            );
            assert!(
                screen.contains("\x1b[?2004l"),
                "paste not restored: {result}"
            );
            assert!(
                screen.contains("\x1b[0m\x1b[?1049l\x1b[?25h"),
                "{fault}: {result}"
            );
            assert!(
                !result.to_string().contains("test-secret"),
                "{fault}: {result}"
            );
            if fault == "restore-error" {
                assert!(
                    result["stderr"]
                        .as_str()
                        .unwrap()
                        .contains("Terminal restoration failed: alternate screen")
                );
            }
        }
    }
    #[test]
    fn comment_title_uses_only_needed_spaces_and_preserves_body_pipes() {
        use ratatui::backend::TestBackend;
        use serde_json::json;
        let config = Config {
            origin: "https://example.backlog.com".parse().unwrap(),
            api_key: "test-secret".into(),
            editor: vec![],
            settings: Default::default(),
        };
        for (width, height) in [(100, 24), (40, 12)] {
            let mut shell = Shell::new(true);
            let value = json!({"id":1,"content":"body | retained","createdUser":{"name":"Author"},"created":"2026-10-26T16:05:08Z","changeLog":[]});
            shell.browse.comment.replace(
                vec![comment::Entry {
                    id: "1".into(),
                    preview: crate::comment::preview(&value, "unknown", &config).unwrap(),
                }],
                comment::Direction::Latest,
            );
            let area = Rect::new(0, 0, width, height);
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            // Enter the right column, then focus Comment through the actual key boundary.
            for key in ['L', 'l'] {
                shell.key(KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE), area);
            }
            for focused in [true, false] {
                if !focused {
                    shell.key(KeyEvent::new(KeyCode::Char('H'), KeyModifiers::NONE), area);
                }
                // At compact width only the focused panel is visible.
                if width == 40 && !focused {
                    continue;
                }
                terminal.draw(|frame| shell.draw(frame)).unwrap();
                let panel = shell.panel_area(area, Focus::Comment);
                let buffer = terminal.backend().buffer();
                let title: String = (panel.x..panel.right())
                    .map(|x| buffer[(x, panel.y)].symbol())
                    .collect();
                let expected = if focused {
                    "╔> Comment═"
                } else {
                    "┌Comment─"
                };
                assert!(title.starts_with(expected), "{title}");
                assert!(!title.contains('|'), "{title}");
            }
            shell.key(KeyEvent::new(KeyCode::Char('L'), KeyModifiers::NONE), area);
            shell.key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE), area);
            terminal.draw(|frame| shell.draw(frame)).unwrap();
            let buffer = terminal.backend().buffer();
            let screen: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
            assert!(screen.contains("body | retained"), "{screen}");
            assert!(
                buffer
                    .content
                    .iter()
                    .all(|cell| (cell.fg, cell.bg) == (Color::Reset, Color::Reset))
            );
        }
    }

    #[test]
    fn comment_fixed_rows_styles_and_body_scroll_share_the_viewport() {
        use ratatui::backend::TestBackend;
        use serde_json::json;
        let config = Config {
            origin: "https://example.backlog.com".parse().unwrap(),
            api_key: "test-secret".into(),
            editor: vec![],
            settings: Default::default(),
        };
        for no_color in [false, true] {
            let mut shell = Shell::new(no_color);
            shell.set_focus(Focus::Comment);
            let value = json!({"id":1,"content":null,"createdUser":{"name":"Author"},"created":"2026-10-26T16:05:08Z","changeLog":[{"field":"description","originalValue":"old\ntail","newValue":"new\ntail"}]});
            shell.browse.comment.replace(
                vec![comment::Entry {
                    id: "1".into(),
                    preview: crate::comment::preview(&value, "", &config).unwrap(),
                }],
                comment::Direction::Latest,
            );
            let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
            terminal.draw(|frame| shell.draw(frame)).unwrap();
            let buffer = terminal.backend().buffer();
            let row = |y| {
                (41..99)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            };
            assert!(row(18).starts_with("Author"));
            assert!(row(19).starts_with("2026/10/27 01:05:08"));
            assert!(row(20).trim().is_empty());
            assert!(row(21).replace(' ', "").starts_with("詳細"));
            assert!(buffer[(41, 18)].modifier.contains(Modifier::BOLD));
            assert_eq!(
                buffer[(41, 18)].fg,
                if no_color { Color::Reset } else { Color::Cyan }
            );
            shell.key(
                KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
                Rect::new(0, 0, 100, 24),
            );
            terminal.draw(|frame| shell.draw(frame)).unwrap();
            let buffer = terminal.backend().buffer();
            assert_eq!(buffer[(41, 18)].symbol(), "A");
            assert_eq!(buffer[(41, 19)].symbol(), "2");
            if no_color {
                let row: String = (41..99).map(|x| buffer[(x, 21)].symbol()).collect();
                assert!(row.starts_with("[-old-]{+new+}"), "{row}");
                for cell in &buffer.content {
                    assert_eq!((cell.fg, cell.bg), (Color::Reset, Color::Reset));
                }
            } else {
                assert_eq!(
                    (buffer[(41, 21)].fg, buffer[(41, 21)].bg),
                    (Color::Rgb(255, 255, 255), Color::Rgb(128, 0, 0))
                );
                assert!(buffer[(41, 21)].modifier.contains(Modifier::CROSSED_OUT));
                assert_eq!(
                    (buffer[(44, 21)].fg, buffer[(44, 21)].bg),
                    (Color::Black, Color::Green)
                );
            }
        }
    }

    #[test]
    fn comment_long_author_and_invalid_date_reflow_safely_on_resize() {
        use ratatui::{backend::TestBackend, text::Line};
        use serde_json::json;
        let config = Config {
            origin: "https://example.backlog.com".parse().unwrap(),
            api_key: "abcd".into(),
            editor: vec![],
            settings: Default::default(),
        };
        for no_color in [false, true] {
            let mut shell = Shell::new(no_color);
            shell.set_focus(Focus::Comment);
            let author = format!("{}e\u{301}👩‍💻TAIL", "日本語".repeat(10));
            let invalid = format!("INVALID{}ENDDATE", "x".repeat(75));
            let value = json!({"id":1,"content":"BODY sentinel","createdUser":{"name":author},"created":invalid,"changeLog":[{"field":"description","originalValue":"ab","newValue":"cd"}]});
            shell.browse.comment.replace(
                vec![comment::Entry {
                    id: "1".into(),
                    preview: crate::comment::preview(&value, "unknown", &config).unwrap(),
                }],
                comment::Direction::Latest,
            );
            let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
            terminal.draw(|frame| shell.draw(frame)).unwrap();
            let buffer = terminal.backend().buffer();
            let header: String = (1..39).map(|x| buffer[(x, 1)].symbol()).collect();
            assert!(header.contains('…'));
            assert!(!header.contains('\u{301}'));
            assert!(
                buffer
                    .content
                    .iter()
                    .all(|cell| !cell.symbol().contains('\x1b'))
            );
            let mut output = String::new();
            for _ in 0..40 {
                terminal.draw(|frame| shell.draw(frame)).unwrap();
                output.extend(
                    terminal
                        .backend()
                        .buffer()
                        .content
                        .iter()
                        .map(|c| c.symbol()),
                );
                shell.key(
                    KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
                    Rect::new(0, 0, 40, 12),
                );
            }
            assert!(
                output.contains("TAIL")
                    && output.contains("ENDDATE")
                    && output.contains("BODY sentinel")
            );
            assert!(!output.contains("abcd"));
            // Width now fits the full author: duplicate author lines must disappear and end clamps.
            let mut wide = Terminal::new(TestBackend::new(200, 60)).unwrap();
            wide.draw(|frame| shell.draw(frame)).unwrap();
            let buffer = wide.backend().buffer();
            assert_eq!(buffer[(81, 52)].symbol(), "B");
            let header: String = (81..199).map(|x| buffer[(x, 49)].symbol()).collect();
            assert!(!header.contains('…'));
            assert!(Line::from(author.as_str()).width() < 118);
        }
    }
    #[test]
    fn comment_warning_and_final_secret_projection_are_safe_in_every_rendered_cell() {
        use ratatui::backend::TestBackend;
        use serde_json::json;
        let config = Config {
            origin: "https://example.backlog.com".parse().unwrap(),
            api_key: "abcd".into(),
            editor: vec![],
            settings: Default::default(),
        };
        for no_color in [false, true] {
            for (old, new) in [
                ("ab".to_owned(), "cd"),
                (format!("HIDDEN{}", "x".repeat(9995)), "x"),
            ] {
                let mut shell = Shell::new(no_color);
                shell.browse.secret = config.api_key.clone();
                shell.set_focus(Focus::Comment);
                let value = json!({"id":1,"content":"visible\u{1b}]52;c;bad\u{7}","createdUser":{"name":"Author"},"created":"2026-10-26T16:05:08Z","changeLog":[{"field":"description","originalValue":old,"newValue":new}]});
                shell.browse.comment.replace(
                    vec![comment::Entry {
                        id: "1".into(),
                        preview: crate::comment::preview(&value, "unknown", &config).unwrap(),
                    }],
                    comment::Direction::Latest,
                );
                let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
                let mut output = String::new();
                for _ in 0..40 {
                    terminal.draw(|frame| shell.draw(frame)).unwrap();
                    let buffer = terminal.backend().buffer();
                    output.extend(buffer.content.iter().map(|c| c.symbol()));
                    if no_color {
                        for cell in &buffer.content {
                            assert_eq!((cell.fg, cell.bg), (Color::Reset, Color::Reset));
                        }
                    }
                    if old.starts_with("HIDDEN") && !no_color {
                        for cell in &buffer.content {
                            if cell.symbol() == "変" {
                                assert_eq!(cell.fg, Color::Yellow);
                            }
                        }
                    }
                    shell.key(
                        KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
                        Rect::new(0, 0, 40, 12),
                    );
                }
                assert!(
                    !output.contains("abcd")
                        && !output.contains("HIDDEN")
                        && !output.contains('\u{1b}')
                        && !output.contains('\u{7}')
                );
                assert!(output.contains("visible"));
                if old.starts_with("HIDDEN") {
                    assert!(output.contains("10,000"));
                }
            }
        }
    }
    #[test]
    fn tui_tabs_remain_visible_and_cannot_reassemble_secrets_in_comment_or_detail_cells() {
        use ratatui::backend::TestBackend;
        use serde_json::json;
        for no_color in [false, true] {
            for (secret, expected) in [("abcd", "ab⇥cd"), ("ab⇥cd", "[REDACTED]")] {
                for source in ["description", "content", "author", "date", "detail"] {
                    let config = Config {
                        origin: "https://example.backlog.com".parse().unwrap(),
                        api_key: secret.into(),
                        editor: vec![],
                        settings: Default::default(),
                    };
                    let mut shell = Shell::new(no_color);
                    shell.browse.secret = secret.into();
                    shell.set_focus(if source == "detail" {
                        Focus::Detail
                    } else {
                        Focus::Comment
                    });
                    let mut value = json!({"id":1,"content":null,"createdUser":{"name":"Author"},"created":"2026-10-26T16:05:08Z","changeLog":[]});
                    match source {
                        "description" => {
                            value["changeLog"] = json!([{"field":"description","originalValue":"ab\tcd","newValue":"ab\tcd"}])
                        }
                        "content" => value["content"] = json!("ab\tcd"),
                        "author" => {
                            value["createdUser"]["name"] =
                                json!(format!("{}ab\tcd", "long ".repeat(12)))
                        }
                        "date" => value["created"] = json!("ab\tcd"),
                        "detail" => shell.browse.detail.replace("ab\tcd".into()),
                        _ => unreachable!(),
                    }
                    if source != "detail" {
                        shell.browse.comment.replace(
                            vec![comment::Entry {
                                id: "1".into(),
                                preview: crate::comment::preview(&value, "unknown", &config)
                                    .unwrap(),
                            }],
                            comment::Direction::Latest,
                        );
                    }
                    let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
                    let mut output = String::new();
                    for _ in 0..12 {
                        terminal.draw(|frame| shell.draw(frame)).unwrap();
                        let buffer = terminal.backend().buffer();
                        output.extend(buffer.content.iter().map(|cell| cell.symbol()));
                        if no_color {
                            assert!(
                                buffer
                                    .content
                                    .iter()
                                    .all(|cell| (cell.fg, cell.bg) == (Color::Reset, Color::Reset))
                            );
                        }
                        shell.key(
                            KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
                            Rect::new(0, 0, 40, 12),
                        );
                    }
                    assert!(!output.contains(secret), "{source}: {output}");
                    assert!(output.contains(expected), "{source}: {output}");
                }
            }
        }
    }

    #[test]
    fn comment_zero_width_graphemes_are_safe_in_fixed_rows_overflow_and_body_cells() {
        use ratatui::backend::TestBackend;
        use serde_json::json;
        for no_color in [false, true] {
            for (secret, expected) in [("abcd", "ab\\u{200b}cd"), ("ab\\u{200b}cd", "[REDACTED]")] {
                let config = Config {
                    origin: "https://example.backlog.com".parse().unwrap(),
                    api_key: secret.into(),
                    editor: vec![],
                    settings: Default::default(),
                };
                for source in [
                    "author",
                    "date",
                    "long author",
                    "long date",
                    "description",
                    "content",
                    "detail",
                ] {
                    let raw = "ab\u{200b}cd";
                    let mut value = json!({"id":1,"content":"e\u{301} 👩‍💻","createdUser":{"name":"Author"},"created":"2026-10-26T16:05:08Z","changeLog":[]});
                    let mut shell = Shell::new(no_color);
                    shell.browse.secret = secret.into();
                    shell.set_focus(if source == "detail" {
                        Focus::Detail
                    } else {
                        Focus::Comment
                    });
                    match source {
                        "author" => {
                            value["createdUser"]["name"] = json!(format!("{raw} e\u{301} 👩‍💻"))
                        }
                        "date" => value["created"] = json!(raw),
                        "long author" => {
                            value["createdUser"]["name"] =
                                json!(format!("{}{raw}", "long ".repeat(12)))
                        }
                        "long date" => value["created"] = json!(format!("{raw}{}", "x".repeat(80))),
                        "description" => {
                            value["changeLog"] =
                                json!([{"field":"description","originalValue":raw,"newValue":raw}])
                        }
                        "content" => value["content"] = json!(format!("{raw} e\u{301} 👩‍💻")),
                        "detail" => shell
                            .browse
                            .detail
                            .replace(format!("{raw} e\u{301} 👩‍💻").into()),
                        _ => unreachable!(),
                    }
                    let before = value.clone();
                    shell.browse.comment.replace(
                        vec![comment::Entry {
                            id: "1".into(),
                            preview: crate::comment::preview(&value, "unknown", &config).unwrap(),
                        }],
                        comment::Direction::Latest,
                    );
                    for (width, height) in [(40, 12), (100, 24)] {
                        shell.key(
                            KeyEvent::new(KeyCode::Home, KeyModifiers::NONE),
                            Rect::new(0, 0, width, height),
                        );
                        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
                        let mut output = String::new();
                        for _ in 0..16 {
                            terminal.draw(|frame| shell.draw(frame)).unwrap();
                            let buffer = terminal.backend().buffer();
                            let cells: String =
                                buffer.content.iter().map(|cell| cell.symbol()).collect();
                            assert!(!cells.contains(secret), "{source}: {cells}");
                            output.push_str(&cells);
                            if no_color {
                                assert!(
                                    buffer
                                        .content
                                        .iter()
                                        .all(|cell| (cell.fg, cell.bg)
                                            == (Color::Reset, Color::Reset))
                                );
                            }
                            shell.key(
                                KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
                                Rect::new(0, 0, width, height),
                            );
                        }
                        assert!(output.contains(expected), "{source}: {output}");
                        assert!(output.contains("e\u{301} 👩‍💻"), "{source}: {output}");
                    }
                    assert_eq!(value, before);
                }
            }
        }
    }

    #[test]
    fn comment_zero_width_author_truncation_rechecks_reconstructed_secret() {
        use ratatui::backend::TestBackend;
        use serde_json::json;
        let secret = format!("{}ab\\u{{200b}}cd…", "x".repeat(25));
        let config = Config {
            origin: "https://example.backlog.com".parse().unwrap(),
            api_key: secret.clone(),
            editor: vec![],
            settings: Default::default(),
        };
        let value = json!({"id":1,"content":"BODY sentinel","createdUser":{"name":format!("{}ab\u{200b}cdTAIL", "x".repeat(25))},"created":"2026-10-26T16:05:08Z","changeLog":[]});
        for no_color in [false, true] {
            let mut shell = Shell::new(no_color);
            shell.browse.secret = secret.clone();
            shell.set_focus(Focus::Comment);
            shell.browse.comment.replace(
                vec![comment::Entry {
                    id: "1".into(),
                    preview: crate::comment::preview(&value, "unknown", &config).unwrap(),
                }],
                comment::Direction::Latest,
            );
            let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
            terminal.draw(|frame| shell.draw(frame)).unwrap();
            let cells: String = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect();
            assert!(!cells.contains(&secret), "{cells}");
            assert!(cells.contains("[REDACTED]"), "{cells}");
        }
    }

    #[test]
    fn comment_changed_trailing_newlines_keep_direction_in_rendered_cells() {
        use ratatui::backend::TestBackend;
        use serde_json::json;
        for (no_color, secret) in [
            (false, "test-secret"),
            (true, "test-secret"),
            (false, "line↵"),
            (true, "line↵"),
        ] {
            for (old, new, color, marker) in [
                ("line\n", "line", Color::Rgb(128, 0, 0), "[-"),
                ("line", "line\n", Color::Green, "{+"),
            ] {
                let config = Config {
                    origin: "https://example.backlog.com".parse().unwrap(),
                    api_key: secret.into(),
                    editor: vec![],
                    settings: Default::default(),
                };
                let value = json!({"id":1,"content":null,"createdUser":{"name":"Author"},"created":"2026-10-26T16:05:08Z","changeLog":[{"field":"description","originalValue":old,"newValue":new}]});
                let mut shell = Shell::new(no_color);
                shell.browse.secret = secret.into();
                shell.set_focus(Focus::Comment);
                shell.browse.comment.replace(
                    vec![comment::Entry {
                        id: "1".into(),
                        preview: crate::comment::preview(&value, "", &config).unwrap(),
                    }],
                    comment::Direction::Latest,
                );
                let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
                terminal.draw(|frame| shell.draw(frame)).unwrap();
                let buffer = terminal.backend().buffer();
                let row: String = (1..39).map(|x| buffer[(x, 5)].symbol()).collect();
                if no_color {
                    assert!(row.starts_with(&format!("line{marker}")), "{row}");
                    let next: String = (1..39).map(|x| buffer[(x, 6)].symbol()).collect();
                    assert!(next.starts_with(if color == Color::Rgb(128, 0, 0) {
                        "-]"
                    } else {
                        "+}"
                    }));
                    assert!(
                        buffer
                            .content
                            .iter()
                            .all(|cell| (cell.fg, cell.bg) == (Color::Reset, Color::Reset))
                    );
                } else if secret == "line↵" {
                    assert!(row.starts_with("[REDACTED]"), "{row}");
                    assert!(!row.contains(secret));
                    assert!((1..39).all(|x| buffer[(x, 5)].bg == Color::Reset));
                } else {
                    assert!(row.starts_with("line↵"), "{row}");
                    let cell = &buffer[(5, 5)];
                    assert_eq!(cell.bg, color);
                    assert_eq!(
                        cell.fg,
                        if color == Color::Rgb(128, 0, 0) {
                            Color::Rgb(255, 255, 255)
                        } else {
                            Color::Black
                        }
                    );
                    assert_eq!(
                        cell.modifier.contains(Modifier::CROSSED_OUT),
                        color == Color::Rgb(128, 0, 0)
                    );
                    for x in 1..5 {
                        assert_eq!(buffer[(x, 5)].bg, Color::Reset);
                    }
                    assert!(!row.contains("[-") && !row.contains("{+"));
                    assert!((1..39).all(|x| buffer[(x, 6)].symbol() == " "));
                }
            }
        }
    }
}
