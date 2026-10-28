//! Read-only project, issue-list and detail browsing.
mod browse;
mod detail;
mod input;
use crate::{Error, config::Config, safety};
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

/// Input and rendering boundary, independent of the terminal and network.
pub struct Shell {
    focus: Focus,
    browse: browse::Browse,
    no_color: bool,
    #[cfg(test)]
    input_check: Option<tests::InputCheck>,
}
impl Shell {
    pub fn new(no_color: bool) -> Self {
        Self {
            focus: Focus::Project,
            browse: browse::Browse::default(),
            no_color,
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
            return input.key(key, area);
        }
        if key.modifiers.is_empty() && key.code == KeyCode::Char('q') {
            return Some(0);
        }
        if area.width < 40 || area.height < 12 {
            return None;
        }
        if self.browse.error_notice.is_some()
            && key.modifiers.is_empty()
            && matches!(key.code, KeyCode::Enter | KeyCode::Esc | KeyCode::Char(' '))
        {
            self.browse.error_notice = None;
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
        if self.focus == Focus::Detail {
            self.browse.detail.key(
                key.code,
                Rect::new(
                    0,
                    0,
                    panel.width.saturating_sub(2),
                    panel.height.saturating_sub(3),
                ),
                &self.browse.secret,
            );
        }
        self.browse.key(key.code, self.focus, rows);
        match key.code {
            KeyCode::Char('h' | 'l') | KeyCode::Tab | KeyCode::BackTab => {
                self.focus = match self.focus {
                    Focus::Project => Focus::Issue,
                    Focus::Issue => Focus::Project,
                    Focus::Detail => Focus::Comment,
                    Focus::Comment => Focus::Detail,
                };
            }
            KeyCode::Enter if self.focus == Focus::Issue => self.focus = Focus::Detail,
            KeyCode::Esc if matches!(self.focus, Focus::Detail | Focus::Comment) => {
                self.focus = Focus::Issue
            }
            _ => {}
        }
        None
    }
    fn event(&mut self, event: Event, area: Rect) -> Option<u8> {
        match event {
            Event::Key(key) => self.key(key, area),
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
            return;
        }
        let compact = area.width < 100 || area.height < 24;
        let rows = Layout::vertical([
            Constraint::Min(0),
            Constraint::Length(if compact { 3 } else { 1 }),
        ])
        .split(area);
        let (navigation, action) = match self.focus {
            Focus::Project => ("h/l Tab: Issue", "j/k: move Enter: select R: reload"),
            Focus::Issue => ("h/l Tab: Project", "j/k p/n Enter:Detail Ctrl+O R"),
            Focus::Detail => (
                "h/l Tab: Comment Esc: Issue",
                "j/k PgUp/Dn b/Space g/G Home/End R",
            ),
            Focus::Comment => ("h/l Tab/Shift+Tab: Detail", "Esc: Issue"),
        };
        let exits = "q: quit | Ctrl+C: interrupt";
        let keys = if compact {
            format!("{navigation}\n{action}\n{exits}")
        } else if action.is_empty() {
            format!("{navigation} | {exits}")
        } else {
            format!("{navigation} | {action} | {exits}")
        };
        frame.render_widget(Paragraph::new(keys), rows[1]);
        if compact {
            self.panel(frame, rows[0], self.focus);
            self.error_popup(frame);
            return;
        }
        let main = Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).split(rows[0]);
        let workspace = format!(
            "{}  | * settings: not implemented",
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
        self.error_popup(frame);
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
            Focus::Detail | Focus::Comment => {
                Layout::vertical([Constraint::Percentage(80), Constraint::Min(0)]).split(columns[1])
                    [if focus == Focus::Detail { 0 } else { 1 }]
            }
        }
    }
    fn panel(&self, frame: &mut Frame<'_>, area: Rect, focus: Focus) {
        let title = match focus {
            Focus::Project => "Project List",
            Focus::Issue => "Issue List",
            Focus::Detail => "Issue Detail",
            Focus::Comment => "Comment",
        };
        let label = if focus == self.focus {
            format!("> {title}")
        } else {
            title.into()
        };
        let mut block = Block::default().borders(Borders::ALL).title(label);
        if focus == self.focus {
            let mut style = Style::default().add_modifier(Modifier::BOLD);
            if !self.no_color {
                style = style.fg(Color::Cyan);
            }
            block = block.border_type(BorderType::Double).border_style(style);
        }
        if focus == Focus::Detail {
            let inner = block.inner(area);
            frame.render_widget(block, area);
            let rows = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(inner);
            frame.render_widget(Paragraph::new(self.browse.detail_status()), rows[0]);
            self.browse.detail.draw(frame, rows[1], &self.browse.secret);
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
            shell.browse.start(&config.api_key);
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

    pub(super) struct InputCheck {
        pub(super) field: SingleLineInput,
        pub(super) focused: bool,
    }
    impl InputCheck {
        fn new() -> Self {
            Self {
                field: SingleLineInput::new(String::new()),
                focused: true,
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
            frame.render_widget(Paragraph::new("Input check (no network)\nTab/Shift+Tab: focus | Esc: leave input\nOutside input: q: quit | Ctrl+C: interrupt"), Rect::new(0, 0, frame.area().width, 3));
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
    fn detail_document_scroll_upper_bound_does_not_overflow() {
        use ratatui::backend::TestBackend;
        let mut shell = Shell::new(true);
        shell.focus = Focus::Detail;
        shell.browse.detail.replace("Row\n".repeat(65_550));
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
}
