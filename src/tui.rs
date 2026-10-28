//! Basic read-only shell. Issue operations and settings editing are not implemented here.
mod input;
use crate::{Error, api::Api, config::Config, safety};
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
    widgets::{Block, BorderType, Borders, Paragraph, Wrap},
};
use serde_json::Value;
use std::{
    io::{self, Write},
    panic::{AssertUnwindSafe, catch_unwind},
    time::Duration,
};
use tokio::{runtime::Builder, task::JoinHandle};

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
    space: Option<String>,
    projects: Vec<String>,
    no_color: bool,
    #[cfg(test)]
    input_check: Option<tests::InputCheck>,
}
impl Shell {
    pub fn new(no_color: bool) -> Self {
        Self {
            focus: Focus::Project,
            space: None,
            projects: vec![],
            no_color,
            #[cfg(test)]
            input_check: None,
        }
    }
    fn loaded(&mut self, space: Value, projects: Value, secret: &str) {
        self.space = Some(safety::text(space["name"].as_str().unwrap(), &[secret]));
        self.projects = projects
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                safety::text(
                    &format!(
                        "{}  {}",
                        p["projectKey"].as_str().unwrap(),
                        p["name"].as_str().unwrap()
                    ),
                    &[secret],
                )
            })
            .collect();
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
        if !key.modifiers.is_empty() && key.modifiers != KeyModifiers::SHIFT {
            return None;
        }
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
            Focus::Project => ("h/l Tab/Shift+Tab: Issue", ""),
            Focus::Issue => ("h/l Tab/Shift+Tab: Project", "Enter: Detail"),
            Focus::Detail => ("h/l Tab/Shift+Tab: Comment", "Esc: Issue"),
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
            return;
        }
        let main = Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).split(rows[0]);
        let workspace = format!(
            "{}  | * settings: not implemented",
            self.space.as_deref().unwrap_or("Loading space...")
        );
        frame.render_widget(
            Paragraph::new(workspace)
                .block(Block::default().borders(Borders::ALL).title("Workspace")),
            main[0],
        );
        let columns = Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(main[1]);
        let left =
            Layout::vertical([Constraint::Percentage(35), Constraint::Min(0)]).split(columns[0]);
        let right =
            Layout::vertical([Constraint::Percentage(80), Constraint::Min(0)]).split(columns[1]);
        for (rect, focus) in [
            (left[0], Focus::Project),
            (left[1], Focus::Issue),
            (right[0], Focus::Detail),
            (right[1], Focus::Comment),
        ] {
            self.panel(frame, rect, focus);
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
        let content = match focus {
            Focus::Project if self.space.is_none() => "Loading projects...".into(),
            Focus::Project if self.projects.is_empty() => "No projects".into(),
            Focus::Project => self.projects.join("\n"),
            _ => "Not implemented (shell only)".into(),
        };
        frame.render_widget(
            Paragraph::new(content)
                .wrap(Wrap { trim: false })
                .block(block),
            area,
        );
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
type Startup = JoinHandle<Result<(Value, Value), Error>>;

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
    let mut pending: Option<Startup> = None;
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
            let api = Api::new(&config)?;
            pending = Some(tokio::spawn(async move {
                let space = api.space().await.map_err(|mut error| {
                    error.message = format!("Initial space retrieval failed: {}", error.message);
                    error
                })?;
                let projects = api.project_list().await.map_err(|mut error| {
                    error.message = format!("Initial projects retrieval failed: {}", error.message);
                    error
                })?;
                Ok((space, projects))
            }));
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
                    if pending.as_ref().is_some_and(JoinHandle::is_finished) {
                        let (space, projects) = pending.take().unwrap().await
                            .map_err(|_| Error::new("terminal", "Startup task failed"))??;
                        shell.loaded(space, projects, &config.api_key);
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
                }
            }
        }
    }))).unwrap_or_else(|_| Err(Error::new("terminal", "TUI stopped after an internal panic")));
    if let Some(task) = pending {
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
