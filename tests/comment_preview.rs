use backlog_cli::{
    comment,
    config::{Config, Settings},
};
use serde_json::{Value, json};

fn config() -> Config {
    Config {
        origin: "https://example.backlog.com".parse().unwrap(),
        api_key: "test-secret".into(),
        editor: vec![],
        settings: Settings::default(),
    }
}
fn item(old: Value, new: Value) -> Value {
    json!({"id":1,"content":null,"createdUser":{"name":"投稿者"},"created":"2026-10-26T16:05:08Z","updated":"2026-10-27T00:00:00Z","changeLog":[{"field":"description","originalValue":old,"newValue":new}]})
}
#[test]
fn preview_has_fixed_author_jst_and_unlabelled_body_without_changing_cli() {
    let mut value = item(json!("old"), json!("new"));
    value["content"] = json!("ordinary BODY text");
    let before = value.clone();
    let preview = comment::preview(&value, "unknown", &config()).unwrap();
    assert_eq!(preview.author, "投稿者");
    assert_eq!(preview.created, "2026/10/27 01:05:08");
    assert!(
        preview
            .text
            .to_string()
            .starts_with("ordinary BODY text\n\n詳細\n")
    );
    assert!(!preview.text.to_string().contains("CHANGES"));
    assert!(!preview.text.to_string().contains("UPDATED"));
    let cli = comment::display(&value, "unknown", &config()).unwrap();
    assert!(cli.contains("CREATED\t2026-10-26T16:05:08Z") && cli.contains("BODY\told -> new"));
    assert_eq!(value, before);
}

#[test]
fn description_inline_diff_keeps_common_unicode_and_marks_only_changed_graphemes() {
    use ratatui::style::{Color, Modifier};
    let preview = comment::preview(
        &item(
            json!("日本語 e\u{301} 👩‍💻 old\nlast\n"),
            json!("日本語 e\u{301} 👩‍💻 new\nlast\n"),
        ),
        "",
        &config(),
    )
    .unwrap();
    assert_eq!(
        preview.text.to_string(),
        "詳細\n日本語 e\u{301} 👩‍💻 oldnew\nlast\n"
    );
    assert_eq!(
        preview.no_color_text.to_string(),
        "詳細\n日本語 e\u{301} 👩‍💻 [-old-]{+new+}\nlast\n"
    );
    let deleted = preview
        .text
        .lines
        .iter()
        .flat_map(|l| &l.spans)
        .find(|s| s.content == "old")
        .unwrap();
    assert_eq!(deleted.style.fg, Some(Color::Rgb(255, 255, 255)));
    assert_eq!(deleted.style.bg, Some(Color::Rgb(128, 0, 0)));
    assert!(deleted.style.add_modifier.contains(Modifier::CROSSED_OUT));
}

#[test]
fn preview_jst_dates_cover_offsets_calendar_rollovers_and_safe_fallbacks() {
    for (raw, expected) in [
        ("2026-10-26T16:05:08Z", "2026/10/27 01:05:08"),
        ("2026-10-26T16:05:10Z", "2026/10/27 01:05:10"),
        ("2026-12-31T23:59:59.999Z", "2027/01/01 08:59:59"),
        ("2024-02-29T23:30:00-03:00", "2024/03/01 11:30:00"),
        ("2024-03-01T00:30:00+14:00", "2024/02/29 19:30:00"),
        ("2026-01-31T20:00:00+00:00", "2026/02/01 05:00:00"),
        ("2026-03-01", "2026-03-01（変換不可）"),
        ("not-a-date", "not-a-date（変換不可）"),
        ("2025-02-29T00:00:00Z", "2025-02-29T00:00:00Z（変換不可）"),
        ("9999-12-31T23:59:59Z", "9999-12-31T23:59:59Z（変換不可）"),
    ] {
        let mut value = item(json!(""), json!(""));
        value["created"] = json!(raw);
        assert_eq!(
            comment::preview(&value, "", &config()).unwrap().created,
            expected,
            "{raw}"
        );
        assert_eq!(value["created"], raw);
    }
}

#[test]
fn description_empty_equal_and_typed_fallbacks_keep_notes_outside_api_values() {
    for (old, new, expected) in [
        (json!(""), json!(""), "詳細（変更なし）\n（空）"),
        (json!("same\n"), json!("same\n"), "詳細（変更なし）\nsame\n"),
        (json!("old"), json!(""), "詳細（空へ変更）\n[-old-]"),
        (json!(""), json!("new"), "詳細（空から追加）\n{+new+}"),
        (Value::Null, json!(""), "詳細: 未設定 -> \"\""),
        (json!("null"), Value::Null, "詳細: \"null\" -> 未設定"),
        (
            json!([1, true]),
            json!({"x":2}),
            "詳細: [1,true] -> {\"x\":2}",
        ),
    ] {
        let value = item(old, new);
        let before = value.clone();
        assert_eq!(
            comment::preview(&value, "", &config())
                .unwrap()
                .no_color_text
                .to_string(),
            expected
        );
        assert_eq!(value, before);
    }
}

#[test]
fn description_scalar_limit_is_before_redaction_and_only_hides_over_limit_pair() {
    for total in [9999, 10000, 10001] {
        // Multiple bytes, combining scalars and newlines count as scalars, not bytes/graphemes.
        let old = format!("SENTINEL{}e\u{301}\n", "日".repeat(total - 12));
        let new = "x";
        assert_eq!(old.chars().count() + new.chars().count(), total);
        let mut value = item(json!(old), json!(new));
        value["content"] = json!("ordinary visible");
        value["changeLog"]
            .as_array_mut()
            .unwrap()
            .push(json!({"field":"futureField","originalValue":null,"newValue":"other visible"}));
        let preview = comment::preview(&value, "unknown", &config()).unwrap();
        for text in [preview.text, preview.no_color_text] {
            let projection = text.to_string();
            assert!(
                projection.contains("ordinary visible") && projection.contains("other visible")
            );
            assert_eq!(
                projection.contains("変更内容が10,000文字を超えるため、差分を表示できません。"),
                total > 10000
            );
            assert_eq!(projection.contains("SENTINEL"), total <= 10000);
        }
    }
    let value = item(json!("test-secret".repeat(1000)), json!(""));
    let preview = comment::preview(&value, "", &config()).unwrap();
    assert!(
        !preview.text.to_string().contains("[REDACTED]"),
        "count must precede redaction"
    );
    assert!(preview.text.to_string().contains("10,000"));
}

#[test]
fn description_final_projection_redacts_reassembled_secrets_and_neutralizes_controls() {
    let mut secret_config = config();
    secret_config.api_key = "abcd".into();
    let preview = comment::preview(&item(json!("ab"), json!("cd")), "", &secret_config).unwrap();
    assert_eq!(preview.text.to_string(), "詳細\n[REDACTED]");
    for text in [preview.text, preview.no_color_text] {
        assert!(!text.to_string().contains("abcd"));
    }
    for raw in [
        "test-secret",
        "A\x1b]52;c;payload\x07",
        "a\x1b[31mb",
        "<&>\n\t",
    ] {
        let preview =
            comment::preview(&item(json!(raw), json!(format!("{raw}new"))), "", &config()).unwrap();
        for text in [preview.text, preview.no_color_text] {
            let text = text.to_string();
            assert!(
                !text.contains("test-secret") && !text.contains('\x1b') && !text.contains('\x07')
            );
        }
    }
}

#[test]
fn description_diff_retains_both_sources_including_whitespace_and_unicode() {
    use ratatui::style::Color;
    let cases = [
        ("abc", "axbc"),
        ("axbc", "abc"),
        ("one\ntwo\nthree", "one\nTWO\nthree\n"),
        (" \t\n", "\t  \n\n"),
        ("line\n", "line"),
        ("line", "line\n"),
        ("\t", "⇥"),
        ("x↵\n", "x↵"),
        ("x↵", "x↵\n"),
        ("x↵\n", "x!\n"),
        ("e\u{301}👩‍💻日本", "e\u{300}👨‍💻日本語"),
        ("a\na\nb\na", "b\na\na\na"),
    ];
    for (old, new) in cases {
        let preview = comment::preview(&item(json!(old), json!(new)), "", &config()).unwrap();
        let mut reconstructed_old = String::new();
        let mut reconstructed_new = String::new();
        let lines = &preview.text.lines[1..];
        for (index, line) in lines.iter().enumerate() {
            let has_break = index + 1 < lines.len();
            let last = line.spans.last().unwrap();
            let changed_break = has_break
                && matches!(last.style.bg, Some(Color::Rgb(128, 0, 0) | Color::Green))
                && last.content.ends_with('↵');
            for (span_index, span) in line.spans.iter().enumerate() {
                let content = if changed_break && span_index + 1 == line.spans.len() {
                    span.content.strip_suffix('↵').unwrap()
                } else {
                    &span.content
                };
                if span.style.bg != Some(Color::Green) {
                    reconstructed_old.push_str(content);
                }
                if span.style.bg != Some(Color::Rgb(128, 0, 0)) {
                    reconstructed_new.push_str(content);
                }
            }
            if has_break {
                if !changed_break || last.style.bg != Some(Color::Green) {
                    reconstructed_old.push('\n');
                }
                if !changed_break || last.style.bg != Some(Color::Rgb(128, 0, 0)) {
                    reconstructed_new.push('\n');
                }
            }
        }
        // Compare display copies, retaining every source newline and visible TAB position.
        assert_eq!(
            reconstructed_old,
            old.replace('\t', "⇥"),
            "old: {old:?} -> {new:?}"
        );
        assert_eq!(
            reconstructed_new,
            new.replace('\t', "⇥"),
            "new: {old:?} -> {new:?}"
        );
        // NO_COLOR keeps newline placement inside the directional delimiters.
        let marked = preview.no_color_text.to_string();
        let body = marked.split_once('\n').unwrap().1;
        let (mut old_out, mut new_out) = (String::new(), String::new());
        let mut rest = body;
        while !rest.is_empty() {
            if let Some(tail) = rest.strip_prefix("[-") {
                let (part, tail) = tail.split_once("-]").unwrap();
                old_out.push_str(part);
                rest = tail;
            } else if let Some(tail) = rest.strip_prefix("{+") {
                let (part, tail) = tail.split_once("+}").unwrap();
                new_out.push_str(part);
                rest = tail;
            } else {
                let ch = rest.chars().next().unwrap();
                old_out.push(ch);
                new_out.push(ch);
                rest = &rest[ch.len_utf8()..];
            }
        }
        assert_eq!(old_out, old.replace('\t', "⇥"));
        assert_eq!(new_out, new.replace('\t', "⇥"));
    }
}

#[test]
fn preview_tabs_are_visible_checked_after_conversion_and_not_equal_to_literal_markers() {
    use ratatui::style::Color;
    for (secret, expected) in [("abcd", "ab⇥cd"), ("ab⇥cd", "[REDACTED]")] {
        let mut config = config();
        config.api_key = secret.into();
        let value = item(json!("ab\tcd"), json!("ab\tcd"));
        let before = value.clone();
        let preview = comment::preview(&value, "unknown", &config).unwrap();
        for text in [preview.text, preview.no_color_text] {
            assert_eq!(text.to_string(), format!("詳細（変更なし）\n{expected}"));
            assert!(!text.to_string().contains(secret));
        }
        assert!(
            comment::display(&value, "unknown", &config)
                .unwrap()
                .contains("ab\tcd -> ab\tcd")
        );
        assert_eq!(value, before);
    }
    let preview = comment::preview(&item(json!("\t"), json!("⇥")), "", &config()).unwrap();
    let spans = &preview.text.lines[1].spans;
    assert!(
        spans
            .iter()
            .any(|s| s.content == "⇥" && s.style.bg == Some(Color::Rgb(128, 0, 0)))
    );
    assert!(
        spans
            .iter()
            .any(|s| s.content == "⇥" && s.style.bg == Some(Color::Green))
    );
    assert_eq!(preview.no_color_text.to_string(), "詳細\n[-⇥-]{+⇥+}");
}

#[test]
fn preview_zero_width_graphemes_cannot_reassemble_secrets_in_rendered_cells() {
    use ratatui::{
        Terminal,
        backend::TestBackend,
        widgets::{Paragraph, Wrap},
    };
    let mut config = config();
    config.api_key = "abcd".into();
    for content in [false, true] {
        let raw = "\u{ff9e}A\n\u{ff9f}A\nab\u{200b}cd e\u{301} 👩‍💻";
        let mut value = item(json!(raw), json!(raw));
        if content {
            value["content"] = json!(raw);
            value["changeLog"] = json!([]);
        }
        let before = value.clone();
        let preview = comment::preview(&value, "unknown", &config).unwrap();
        for text in [preview.text, preview.no_color_text] {
            let mut terminal = Terminal::new(TestBackend::new(80, 12)).unwrap();
            terminal
                .draw(|frame| {
                    frame.render_widget(
                        Paragraph::new(text).wrap(Wrap { trim: false }),
                        frame.area(),
                    );
                })
                .unwrap();
            let cells: String = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect();
            assert!(!cells.contains("abcd"), "{cells}");
            assert!(cells.contains("ﾞA"), "{cells}");
            assert!(cells.contains("ﾟA"), "{cells}");
            assert!(cells.contains("ab\\u{200b}cd e\u{301} 👩‍💻"), "{cells}");
        }
        assert!(
            comment::display(&value, "unknown", &config)
                .unwrap()
                .contains(raw)
        );
        assert_eq!(value, before);
    }
}

#[test]
fn preview_changed_newlines_have_directional_cells_without_losing_line_breaks() {
    use ratatui::style::{Color, Modifier};
    for (old, new, color, marked) in [
        ("line\n", "line", Color::Rgb(128, 0, 0), "line[-\n-]"),
        ("line", "line\n", Color::Green, "line{+\n+}"),
    ] {
        let value = item(json!(old), json!(new));
        let before = value.clone();
        let preview = comment::preview(&value, "", &config()).unwrap();
        assert_eq!(preview.text.to_string(), "詳細\nline↵\n");
        let marker = preview.text.lines[1]
            .spans
            .iter()
            .find(|s| s.content == "↵")
            .unwrap();
        assert_eq!(marker.style.bg, Some(color));
        assert_eq!(
            marker.style.add_modifier.contains(Modifier::CROSSED_OUT),
            color == Color::Rgb(128, 0, 0)
        );
        assert_eq!(preview.no_color_text.to_string(), format!("詳細\n{marked}"));
        assert_eq!(value, before);
    }
    let mut config = config();
    config.api_key = "line↵".into();
    let preview = comment::preview(&item(json!("line\n"), json!("line")), "", &config).unwrap();
    assert_eq!(preview.text.to_string(), "詳細\n[REDACTED]\n");
    assert!(!preview.no_color_text.to_string().contains("line↵"));
}

#[test]
fn preview_only_top_level_null_is_unset_for_known_description_and_unknown_fields() {
    let huge: Value = serde_json::from_str("123456789012345678901234567890").unwrap();
    for (field, label) in [
        ("startDate", "START DATE"),
        ("assigner", "ASSIGNEE"),
        ("description", "詳細"),
        ("futureField", "futureField"),
    ] {
        for (other, ordinary, typed) in [
            (Value::Null, "未設定", "未設定"),
            (json!("null"), "null", "\"null\""),
            (json!(""), "", "\"\""),
            (json!("2026-11-01"), "2026-11-01", "\"2026-11-01\""),
            (json!(0), "0", "0"),
            (json!(false), "false", "false"),
            (json!({"x":null}), "{\"x\":null}", "{\"x\":null}"),
            (json!([1, null]), "[1,null]", "[1,null]"),
            (
                huge.clone(),
                "123456789012345678901234567890",
                "123456789012345678901234567890",
            ),
        ] {
            let rendered = if field == "description" {
                typed
            } else {
                ordinary
            };
            for null_first in [true, false] {
                let (old, new, expected) = if null_first {
                    (
                        Value::Null,
                        other.clone(),
                        format!("{label}: 未設定 -> {rendered}"),
                    )
                } else {
                    (
                        other.clone(),
                        Value::Null,
                        format!("{label}: {rendered} -> 未設定"),
                    )
                };
                let mut value = item(old, new);
                value["content"] = json!("ordinary | body");
                value["changeLog"][0]["field"] = json!(field);
                let before = value.clone();
                let preview = comment::preview(&value, "unknown", &config()).unwrap();
                for text in [preview.text, preview.no_color_text] {
                    assert_eq!(text.to_string(), format!("ordinary | body\n\n{expected}"));
                }
                assert_eq!(value, before);
            }
        }
    }
    // Recheck the final projection after adding the display-only unset value.
    let mut config = config();
    config.api_key = "未設定 -> ".into();
    let preview = comment::preview(&item(Value::Null, json!("")), "", &config).unwrap();
    for text in [preview.text, preview.no_color_text] {
        assert_eq!(text.to_string(), "詳細: [REDACTED]\"\"");
    }
}
