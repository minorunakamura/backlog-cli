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
fn shell_full_layout_gives_detail_fourteen_rows_and_comment_six_rows() {
    let shell = Shell::new(true);
    let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
    terminal.draw(|frame| shell.draw(frame)).unwrap();
    let buffer = terminal.backend().buffer();
    // Right column spans rows 3..23: Detail gets 14 rows, Comment gets 6.
    for (row, corner) in [(3, "┌"), (16, "└"), (17, "┌"), (22, "└")] {
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

#[test]
fn shell_uppercase_region_keys_restore_last_panels_and_enter_escape_update_memory() {
    for (w, h) in [(100, 24), (40, 12)] {
        for modifiers in [KeyModifiers::NONE, KeyModifiers::SHIFT] {
            let mut shell = Shell::new(true);
            for (code, expected) in [
                (KeyCode::Char('H'), "> Project List"),
                (KeyCode::Char('L'), "> Issue Detail"),
                (KeyCode::Char('L'), "> Issue Detail"),
                (KeyCode::Char('l'), "> Comment"),
                (KeyCode::Char('H'), "> Project List"),
                (KeyCode::Char('H'), "> Project List"),
                (KeyCode::Char('L'), "> Comment"),
                (KeyCode::Esc, "> Issue List"),
                (KeyCode::Char('L'), "> Comment"),
                (KeyCode::Char('H'), "> Issue List"),
                (KeyCode::Enter, "> Issue Detail"),
                (KeyCode::Char('H'), "> Issue List"),
                (KeyCode::Char('L'), "> Issue Detail"),
                (KeyCode::Tab, "> Comment"),
                (KeyCode::Char('H'), "> Issue List"),
                (KeyCode::BackTab, "> Project List"),
                (KeyCode::Char('L'), "> Comment"),
                (KeyCode::BackTab, "> Issue Detail"),
                (KeyCode::Char('H'), "> Project List"),
            ] {
                let mods = if matches!(code, KeyCode::Char('H' | 'L')) {
                    modifiers
                } else if code == KeyCode::BackTab {
                    KeyModifiers::SHIFT
                } else {
                    KeyModifiers::NONE
                };
                assert_eq!(key(&mut shell, code, mods, w, h), None);
                assert!(
                    screen(&shell, w, h).contains(expected),
                    "{code:?} {mods:?}: {}",
                    screen(&shell, w, h)
                );
            }
        }
    }
}

#[test]
fn shell_region_guidance_fits_full_and_minimum_sizes() {
    for (w, h) in [(100, 24), (40, 12)] {
        let mut shell = Shell::new(true);
        for (code, action) in [
            (KeyCode::Char('H'), "Enter:select"),
            (KeyCode::Tab, "Enter:Detail"),
            (KeyCode::Char('L'), "Esc:Issue"),
            (KeyCode::Tab, "Esc:Issue"),
        ] {
            key(&mut shell, code, KeyModifiers::NONE, w, h);
            let text = screen(&shell, w, h);
            for help in [
                "H:Left",
                "L:Right",
                "h/l",
                "Tab/S-Tab",
                action,
                "q:quit",
                "Ctrl+C",
            ] {
                assert!(text.contains(help), "{w}x{h} missing {help}: {text}");
            }
        }
    }
}

#[test]
fn shell_rejected_region_events_and_tiny_resize_preserve_focus_and_memory() {
    use crossterm::event::KeyEventKind;
    for no_color in [false, true] {
        let mut shell = Shell::new(no_color);
        key(&mut shell, KeyCode::Tab, KeyModifiers::NONE, 40, 12);
        key(&mut shell, KeyCode::Enter, KeyModifiers::NONE, 40, 12);
        key(&mut shell, KeyCode::Tab, KeyModifiers::NONE, 40, 12);
        key(&mut shell, KeyCode::Char('H'), KeyModifiers::NONE, 40, 12);
        let before = screen(&shell, 100, 24);
        for code in [KeyCode::Char('H'), KeyCode::Char('L')] {
            for modifiers in [
                KeyModifiers::CONTROL,
                KeyModifiers::ALT,
                KeyModifiers::CONTROL | KeyModifiers::SHIFT,
                KeyModifiers::ALT | KeyModifiers::SHIFT,
                KeyModifiers::SUPER,
            ] {
                shell.key(KeyEvent::new(code, modifiers), Rect::new(0, 0, 100, 24));
                assert_eq!(screen(&shell, 100, 24), before);
            }
            for kind in [KeyEventKind::Repeat, KeyEventKind::Release] {
                for modifiers in [KeyModifiers::NONE, KeyModifiers::SHIFT] {
                    shell.key(
                        KeyEvent::new_with_kind(code, modifiers, kind),
                        Rect::new(0, 0, 100, 24),
                    );
                    assert_eq!(screen(&shell, 100, 24), before);
                }
            }
            for (w, h) in [(39, 12), (40, 11)] {
                key(&mut shell, code, KeyModifiers::NONE, w, h);
                screen(&shell, w, h);
                assert_eq!(screen(&shell, 100, 24), before);
            }
        }
        key(&mut shell, KeyCode::Char('L'), KeyModifiers::NONE, 40, 12);
        assert!(screen(&shell, 40, 12).contains("> Comment"));
        key(&mut shell, KeyCode::Char('H'), KeyModifiers::SHIFT, 100, 24);
        assert!(screen(&shell, 100, 24).contains("> Issue List"));
        assert_eq!(
            key(
                &mut shell,
                KeyCode::Char('c'),
                KeyModifiers::CONTROL,
                40,
                12
            ),
            Some(130)
        );
        assert_eq!(
            key(&mut shell, KeyCode::Char('q'), KeyModifiers::NONE, 40, 12),
            Some(0)
        );
    }
}

#[test]
fn workspace_settings_selector_cycles_cancels_and_preserves_focus() {
    let mut shell = Shell::new(true);
    key(&mut shell, KeyCode::Char('L'), KeyModifiers::SHIFT, 100, 24);
    let before = screen(&shell, 100, 24);
    key(&mut shell, KeyCode::Char('*'), KeyModifiers::SHIFT, 100, 24);
    assert!(screen(&shell, 100, 24).contains("> Common config"));
    for code in [
        KeyCode::Up,
        KeyCode::Char('k'),
        KeyCode::Down,
        KeyCode::Char('j'),
    ] {
        key(&mut shell, code, KeyModifiers::NONE, 100, 24);
        assert!(screen(&shell, 100, 24).contains("> cwd settings"));
        key(&mut shell, code, KeyModifiers::NONE, 100, 24);
        assert!(screen(&shell, 100, 24).contains("> Common config"));
    }
    assert_eq!(
        key(&mut shell, KeyCode::Char('q'), KeyModifiers::NONE, 100, 24),
        None
    );
    key(&mut shell, KeyCode::Esc, KeyModifiers::NONE, 100, 24);
    assert_eq!(screen(&shell, 100, 24), before);
    key(&mut shell, KeyCode::Char('*'), KeyModifiers::NONE, 40, 12);
    assert!(screen(&shell, 40, 12).contains("Settings editor"));
    assert_eq!(
        key(
            &mut shell,
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
            40,
            12
        ),
        Some(130)
    );
}
