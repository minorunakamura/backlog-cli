use backlog_cli::tui::Shell;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Terminal,
    backend::TestBackend,
    layout::Rect,
    style::{Color, Modifier},
};

fn screen(shell: &Shell, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| shell.draw(frame)).unwrap();
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol())
        .collect()
}
fn key(shell: &mut Shell, code: KeyCode, modifiers: KeyModifiers, w: u16, h: u16) -> Option<u8> {
    shell.key(KeyEvent::new(code, modifiers), Rect::new(0, 0, w, h))
}

#[test]
fn shell_shows_all_five_panels_only_at_the_full_layout_boundary() {
    let shell = Shell::new(true);
    let full = screen(&shell, 100, 24);
    for title in [
        "Workspace",
        "> Project List",
        "Issue List",
        "Issue Detail",
        "Comment",
    ] {
        assert!(full.contains(title), "{title}: {full}");
    }
    for (w, h) in [(99, 24), (100, 23), (40, 12)] {
        let compact = screen(&shell, w, h);
        assert!(compact.contains("> Project List"));
        for hidden in ["Workspace", "Issue List", "Issue Detail", "Comment"] {
            assert!(!compact.contains(hidden), "{w}x{h}: {compact}");
        }
        assert!(
            compact.contains("Ctrl+C"),
            "safe exit must stay visible at {w}x{h}: {compact}"
        );
    }
}

#[test]
fn shell_full_layout_gives_detail_sixteen_rows_and_comment_four_rows() {
    let shell = Shell::new(true);
    let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
    terminal.draw(|frame| shell.draw(frame)).unwrap();
    let buffer = terminal.backend().buffer();
    // Right column spans rows 3..23: Detail gets 16 rows, Comment gets 4.
    for (row, corner) in [(3, "┌"), (18, "└"), (19, "┌"), (22, "└")] {
        assert_eq!(buffer[(40, row)].symbol(), corner, "row {row}");
    }
}

#[test]
fn shell_focus_reaches_empty_right_panels_and_returns_without_claiming_issue_selection() {
    let mut shell = Shell::new(true);
    for code in [
        KeyCode::Char('h'),
        KeyCode::Char('l'),
        KeyCode::Tab,
        KeyCode::BackTab,
    ] {
        let mods = if code == KeyCode::BackTab {
            KeyModifiers::SHIFT
        } else {
            KeyModifiers::NONE
        };
        key(&mut shell, code, mods, 40, 12);
        assert!(screen(&shell, 40, 12).contains("> Issue List"));
        key(&mut shell, code, mods, 40, 12);
        assert!(screen(&shell, 40, 12).contains("> Project List"));
    }
    key(&mut shell, KeyCode::Tab, KeyModifiers::NONE, 40, 12);
    key(&mut shell, KeyCode::Enter, KeyModifiers::NONE, 40, 12);
    assert!(screen(&shell, 40, 12).contains("> Issue Detail"));
    key(&mut shell, KeyCode::Char('l'), KeyModifiers::NONE, 40, 12);
    assert!(screen(&shell, 40, 12).contains("> Comment"));
    key(&mut shell, KeyCode::Esc, KeyModifiers::NONE, 40, 12);
    let issue = screen(&shell, 40, 12);
    assert!(issue.contains("> Issue List") && issue.contains("Select a project"));
    assert!(
        !issue.contains("Cancelled"),
        "Esc without a read is navigation only"
    );
    for unimplemented in ["search", "create", "refresh", "editor", "Ctrl+A"] {
        assert!(!issue.contains(unimplemented));
    }
}

#[test]
fn shell_tiny_sizes_render_without_panicking_and_cannot_change_focus() {
    let mut shell = Shell::new(true);
    for (w, h) in [(39, 12), (40, 11), (1, 1), (0, 0), (0, 12), (40, 0)] {
        screen(&shell, w, h);
        if w >= 39 && h >= 11 {
            assert!(screen(&shell, w, h).contains("40x12"));
        }
        for code in [
            KeyCode::Tab,
            KeyCode::Enter,
            KeyCode::Char('h'),
            KeyCode::Esc,
            KeyCode::Char('c'),
            KeyCode::Char('*'),
        ] {
            assert_eq!(key(&mut shell, code, KeyModifiers::NONE, w, h), None);
        }
        assert!(screen(&shell, 40, 12).contains("> Project List"));
        assert_eq!(
            key(&mut shell, KeyCode::Char('q'), KeyModifiers::NONE, w, h),
            Some(0)
        );
        assert_eq!(
            key(&mut shell, KeyCode::Char('c'), KeyModifiers::CONTROL, w, h),
            Some(130)
        );
    }
}

#[test]
fn shell_no_color_keeps_focus_visible_without_any_color() {
    let shell = Shell::new(true);
    let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
    terminal.draw(|frame| shell.draw(frame)).unwrap();
    for cell in &terminal.backend().buffer().content {
        assert_eq!(cell.fg, Color::Reset);
        assert_eq!(cell.bg, Color::Reset);
    }
    assert!(
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .any(|cell| cell.modifier.contains(Modifier::BOLD))
    );
    assert!(screen(&shell, 100, 24).contains("> Project List"));
}
