use backlog_cli::tui::SingleLineInput;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Terminal,
    backend::{Backend, TestBackend},
    layout::Rect,
};

const SCREEN: Rect = Rect::new(0, 0, 40, 12);
fn key(input: &mut SingleLineInput, code: KeyCode) {
    input.event(&Event::Key(KeyEvent::new(code, KeyModifiers::NONE)), SCREEN);
}
fn draw(
    input: &SingleLineInput,
    width: u16,
    focused: bool,
    secrets: &[&str],
) -> (String, (u16, u16)) {
    let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
    terminal
        .draw(|frame| input.draw(frame, Rect::new(2, 3, width, 1), focused, secrets))
        .unwrap();
    let text = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol())
        .collect();
    let cursor = terminal.backend_mut().get_cursor_position().unwrap();
    (text, (cursor.x, cursor.y))
}

#[test]
fn single_line_input_edits_graphemes_and_places_the_real_cursor_in_display_columns() {
    let mut input = SingleLineInput::new("日本e\u{301}👩‍💻".into());
    let (text, cursor) = draw(&input, 20, true, &[]);
    assert!(text.contains("日 本 "));
    assert_eq!(cursor, (9, 3));
    key(&mut input, KeyCode::Backspace);
    assert_eq!(input.value(), "日本e\u{301}");
    key(&mut input, KeyCode::Left);
    assert_eq!(draw(&input, 20, true, &[]).1, (6, 3));
    key(&mut input, KeyCode::Delete);
    assert_eq!(input.value(), "日本");
    key(&mut input, KeyCode::Home);
    key(&mut input, KeyCode::Delete);
    assert_eq!(input.value(), "本");
    key(&mut input, KeyCode::End);
    assert_eq!(draw(&input, 20, true, &[]).1, (4, 3));
}

#[test]
fn single_line_input_normalizes_paste_without_trimming_or_losing_the_insertion_position() {
    let mut input = SingleLineInput::new("前後".into());
    key(&mut input, KeyCode::Left);
    input.event(&Event::Paste("日本\r\ne\u{301}\r👩‍💻\n\t ".into()), SCREEN);
    assert_eq!(input.value(), "前日本 e\u{301} 👩‍💻   後");
    key(&mut input, KeyCode::Char('!'));
    assert_eq!(input.value(), "前日本 e\u{301} 👩‍💻   !後");
}

#[test]
fn single_line_input_aligns_joined_graphemes_after_paste_and_character_insertion() {
    for paste in [true, false] {
        for (initial, inserted, joined, suffix, columns) in [
            ("\u{301}b", "a", "a\u{301}", "b", 1),
            ("💻", "👩‍", "👩‍💻", "", 2),
            ("🇯", "🇵", "🇵🇯", "", 2),
        ] {
            let mut input = SingleLineInput::new(initial.into());
            key(&mut input, KeyCode::Home);
            if paste {
                input.event(&Event::Paste(inserted.into()), SCREEN);
            } else {
                for c in inserted.chars() {
                    key(&mut input, KeyCode::Char(c));
                }
            }
            let expected = format!("{joined}{suffix}");
            assert_eq!(input.value(), expected);
            assert_eq!(draw(&input, 14, true, &[]).1, (2 + columns, 3));
            assert_eq!(draw(&input, 1, true, &[]).1, (2, 3));
            draw(&input, 1, false, &[]);
            assert_eq!(draw(&input, 14, true, &[]).1, (2 + columns, 3));
            assert_eq!(input.value(), expected);
            key(&mut input, KeyCode::Backspace);
            assert_eq!(input.value(), suffix);
            input.event(&Event::Paste(joined.into()), SCREEN);
            key(&mut input, KeyCode::Left);
            assert_eq!(draw(&input, 14, true, &[]).1, (2, 3));
            key(&mut input, KeyCode::Right);
            assert_eq!(draw(&input, 14, true, &[]).1, (2 + columns, 3));
            key(&mut input, KeyCode::Left);
            key(&mut input, KeyCode::Delete);
            assert_eq!(input.value(), suffix);
        }
    }
}

#[test]
fn single_line_input_aligns_the_cursor_when_deletion_joins_neighboring_graphemes() {
    for delete in [KeyCode::Delete, KeyCode::Backspace] {
        let mut input = SingleLineInput::new("🇵x🇯".into());
        key(&mut input, KeyCode::Home);
        key(&mut input, KeyCode::Right);
        if delete == KeyCode::Backspace {
            key(&mut input, KeyCode::Right);
        }
        key(&mut input, delete);
        assert_eq!(input.value(), "🇵🇯");
        assert_eq!(draw(&input, 14, true, &[]).1, (4, 3));
        assert_eq!(draw(&input, 1, true, &[]).1, (2, 3));
        key(&mut input, KeyCode::Backspace);
        assert_eq!(input.value(), "");
    }
}

#[test]
fn single_line_input_preserves_control_text_and_cut_yank_state_while_aligning_the_cursor() {
    for control in ["\x1b", "\r\n", "\u{009b}"] {
        let mut input = SingleLineInput::new(format!("{control}\u{301}b"));
        key(&mut input, KeyCode::Home);
        key(&mut input, KeyCode::Right);
        key(&mut input, KeyCode::Char('a'));
        assert_eq!(input.value(), format!("{control}a\u{301}b"));
        assert_eq!(draw(&input, 1, true, &[]).1, (2, 3));
        key(&mut input, KeyCode::Backspace);
        assert_eq!(input.value(), format!("{control}b"));
        key(&mut input, KeyCode::Char('x'));
        key(&mut input, KeyCode::Delete);
        assert_eq!(input.value(), format!("{control}x"));
    }
    let mut input = SingleLineInput::new("one two three".into());
    for c in ['w', 'w', 'y'] {
        input.event(
            &Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)),
            SCREEN,
        );
    }
    assert_eq!(input.value(), "one two three");
}

#[test]
fn single_line_input_masks_the_entire_value_and_pins_the_cursor_even_for_offscreen_hazards() {
    for value in [
        format!("secret{}visible", "日".repeat(100)),
        format!("\x1b]52;c;clipboard\x07{}visible", "日".repeat(100)),
        "safe\u{009b}31m".into(),
        "safe\r\n\tend".into(),
    ] {
        let mut input = SingleLineInput::new(value.clone());
        for code in [KeyCode::End, KeyCode::Home, KeyCode::Right] {
            key(&mut input, code);
            let (text, cursor) = draw(&input, 14, true, &["", "secret"]);
            assert!(text.contains("[REDACTED]"), "{text:?}");
            assert!(!text.contains("visible") && !text.contains("safe"));
            assert!(!text.contains('\x1b') && !text.contains("secret"));
            assert_eq!(cursor, (2, 3));
            assert_eq!(input.value(), value);
        }
        assert!(!draw(&input, 14, false, &["secret"]).0.contains("visible"));
    }
    let input = SingleLineInput::new("ordinary".into());
    assert!(draw(&input, 14, true, &[""]).0.contains("ordinary"));
}

#[test]
fn single_line_input_preserves_long_unicode_values_through_scroll_resize_and_focus_changes() {
    let value = format!("{}e\u{301}👩‍💻終", "日".repeat(33_000));
    let mut input = SingleLineInput::new(String::new());
    input.event(&Event::Paste(value.clone()), SCREEN);
    for (width, expected_x) in [(14, 15), (6, 7), (1, 2), (14, 15)] {
        let (text, cursor) = draw(&input, width, true, &[]);
        if width > 1 {
            assert!(text.contains("終"), "last grapheme hidden at width {width}");
        }
        assert_eq!(cursor, (expected_x, 3));
        draw(&input, width, false, &[]);
        assert_eq!(input.value(), value);
    }
    let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
    terminal
        .draw(|f| input.draw(f, Rect::new(2, 3, 10, 1), true, &[]))
        .unwrap();
    assert!(terminal.backend().cursor_visible());
    terminal
        .draw(|f| input.draw(f, Rect::new(2, 3, 10, 1), false, &[]))
        .unwrap();
    assert!(!terminal.backend().cursor_visible());
}

#[test]
fn single_line_input_does_not_edit_or_show_a_cursor_on_tiny_terminals() {
    let mut input = SingleLineInput::new("保持".into());
    for (width, height) in [(39, 12), (40, 11), (0, 0)] {
        let area = Rect::new(0, 0, width, height);
        input.event(&Event::Paste("変更".into()), area);
        input.event(
            &Event::Key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)),
            area,
        );
        assert_eq!(input.value(), "保持");
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| input.draw(f, area, true, &[])).unwrap();
        assert!(!terminal.backend().cursor_visible());
    }
}

#[test]
fn single_line_input_masks_dropped_graphemes_without_changing_raw_edits_or_cursor_mapping() {
    for value in [
        "test-\u{200b}secret".to_owned(),
        format!("test-\u{200b}secret{}visible", "日".repeat(100)),
        "test-secret".to_owned(),
        "test-\x1bsecret".to_owned(),
        "test-\tsecret".to_owned(),
        "\u{301}visible".to_owned(),
    ] {
        let mut input = SingleLineInput::new(value.clone());
        for code in [KeyCode::End, KeyCode::Home, KeyCode::Right] {
            key(&mut input, code);
            for width in [14, 6, 1] {
                let (text, cursor) = draw(&input, width, true, &["test-secret"]);
                assert!(
                    text.contains(&"[REDACTED]"[..usize::from(width.min(10))]),
                    "{text:?}"
                );
                assert!(
                    !text.contains("test-secret") && !text.contains("visible"),
                    "{text:?}"
                );
                assert_eq!(cursor, (2, 3));
                assert_eq!(input.value(), value);
                assert!(
                    !draw(&input, width, false, &["test-secret"])
                        .0
                        .contains("visible")
                );
            }
        }
        key(&mut input, KeyCode::End);
        key(&mut input, KeyCode::Char('!'));
        assert_eq!(input.value(), format!("{value}!"));
        key(&mut input, KeyCode::Backspace);
        assert_eq!(input.value(), value);
    }
}
