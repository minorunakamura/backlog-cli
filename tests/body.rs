use backlog_cli::body;

#[test]
fn display_keeps_raw_body_for_unsupported_rules_without_changing_the_source() {
    let raw = " # 見出し é 👩‍💻\n<b>&#27;</b> [[unknown]]\n```\n*code*\n```\tsecret\u{1b}]52;c;bad\u{7}\r\u{9b}".to_owned();
    let original = raw.clone();
    for rule in ["future-rule", ""] {
        assert_eq!(
            body::display(&raw, rule, &["secret"]),
            " # 見出し é 👩‍💻\n<b>&#27;</b> [[unknown]]\n```\n*code*\n```\t[REDACTED]\\u{1b}]52;c;bad\\u{7}\\u{d}\\u{9b}"
        );
        assert_eq!(raw, original);
    }
    assert_eq!(body::display("", "markdown", &[]), "");
}

#[test]
fn markdown_headings_and_nested_emphasis_keep_lines_and_unicode() {
    let raw = "# 見出し é 👩‍💻\n\n本文 **強調と *斜体***\n通常\t改行\n\n";
    assert_eq!(
        body::display(raw, "markdown", &[]),
        "見出し é 👩‍💻\n\n本文 強調と 斜体\n通常\t改行\n\n"
    );
    assert_eq!(
        raw,
        "# 見出し é 👩‍💻\n\n本文 **強調と *斜体***\n通常\t改行\n\n"
    );
}

#[test]
fn markdown_lists_checklists_quotes_and_code_protect_literal_syntax() {
    let raw = "## 作業 ##\n\n- [x] **完了**\n  * [ ] 未完了\n\n3. 順序\n\n> 引用 *本文*\n> 次の行\n\n```md\n# code **literal** &amp; [[macro]]\n\t- [x] 保持\n```\n\n`**inline** &amp;` \\*escape\\*\n\n別見出し\n===\n";
    assert_eq!(
        body::display(raw, "markdown", &[]),
        "作業\n\n• ☑ 完了\n  • ☐ 未完了\n\n3. 順序\n\n> 引用 本文\n> 次の行\n\n```md\n# code **literal** &amp; [[macro]]\n\t- [x] 保持\n```\n\n**inline** &amp; \\*escape\\*\n\n別見出し\n"
    );
}

#[test]
fn markdown_links_images_and_references_are_visible_without_losing_definitions() {
    let raw = "[**表示**](https://example.test/a?x=1&amp;y=2 \"題名\")\n![代替 *文字*](https://example.test/image.png)\n[参照][ref] <https://example.test/auto> [未定義][missing]\n\n[ref]: https://example.test/ref \"参照題名\"\n\n[[Wiki]] #contents() &color(red){未知};\n";
    assert_eq!(
        body::display(raw, "markdown", &[]),
        "表示 (https://example.test/a?x=1&y=2) \"題名\"\n代替 文字 (https://example.test/image.png)\n参照 (https://example.test/ref) \"参照題名\" https://example.test/auto [未定義][missing]\n\n[ref]: https://example.test/ref \"参照題名\"\n\n[[Wiki]] #contents() &color(red){未知};\n"
    );
}

#[test]
fn markdown_safety_runs_after_entity_decoding_and_formatting_without_touching_source() {
    let raw = "# 安全 **test**-secret\n\n&#x1b;]52;c;bad&#7; &#13; &#x9b; test&#45;secret &amp;\n\n[名](https://example.test/test&#45;secret) ![test&#45;secret](https://example.test/a)\n\n<b>&#27;HTML test-secret</b>\n\n```md\n&#27; **test-secret**\n```\n\n    &amp; **literal**\n\n\\*保護\\* \\# 見出しではない\n".to_owned();
    let original = raw.clone();
    assert_eq!(
        body::display(&raw, "markdown", &["test-secret"]),
        "安全 [REDACTED]\n\n\\u{1b}]52;c;bad\\u{7} \\u{d} \\u{9b} [REDACTED] &\n\n名 (https://example.test/[REDACTED]) [REDACTED] (https://example.test/a)\n\n<b>\\u{1b}HTML [REDACTED]</b>\n\n```md\n&#27; **[REDACTED]**\n```\n\n    &amp; **literal**\n\n\\*保護\\* \\# 見出しではない\n"
    );
    assert_eq!(raw, original);
    assert_eq!(
        body::display("&#27;", "markdown", &["\\u{1b}"]),
        "[REDACTED]"
    );
}

#[test]
fn markdown_tables_show_cells_and_fall_back_when_columns_would_be_lost() {
    let raw = "| 項目 | 値 |\n| :--- | ---: |\n| **日本語** | `*code*` |\n| é 👩‍💻 | A\\|B |\n\n| a | b |\n| --- | --- |\n| one | two | **余分な列** |\n\n<div><table><tr><td colspan=\"2\">複雑 &amp; &#27;</td></tr></table></div>\n";
    assert_eq!(
        body::display(raw, "markdown", &[]),
        "項目\t値\n日本語\t*code*\né 👩‍💻\tA\\|B\n\n| a | b |\n| --- | --- |\n| one | two | **余分な列** |\n\n<div><table><tr><td colspan=\"2\">複雑 &amp; &#27;</td></tr></table></div>\n"
    );
}

#[test]
fn markdown_compound_headings_keep_all_lines_and_closing_marker_edge_cases() {
    for (raw, expected) in [
        ("# **強調** #\n", "強調\n"),
        ("# ###\n", "\n"),
        ("#foo\n----\n", "#foo\n"),
        ("一行目\n二行目\n===\n", "一行目\n二行目\n"),
        ("> 一行目\n> 二行目\n> ===\n", "> 一行目\n> 二行目\n"),
        ("# foo\\#\n", "foo\\#\n"),
        ("# title\t###\n", "title\n"),
    ] {
        assert_eq!(body::display(raw, "markdown", &[]), expected, "{raw:?}");
    }
}

#[test]
fn markdown_indented_lists_and_non_atx_setext_headings_keep_content() {
    assert_eq!(
        body::display(" * **項目**\n\n####### text\n===\n", "markdown", &[]),
        " • 項目\n\n####### text\n"
    );
}

#[test]
fn backlog_headings_lists_emphasis_quotes_and_line_breaks_keep_unicode() {
    let raw = "* 見出し é 👩‍💻\n** 小見出し\n*** 詳細\n- ''強調'' と '''斜体''' %%取消%%\n-- 子\n--- 孫\n- [ ] 未完了\n- [x] 完了\n+ 最初\n+ 次\n> 引用 ''本文''\n{quote}\n引用&br;次の行\n{/quote}\n通常\t本文\n\n";
    let original = raw.to_owned();
    assert_eq!(
        body::display(raw, "backlog", &[]),
        "見出し é 👩‍💻\n小見出し\n詳細\n• 強調 と 斜体 取消\n  • 子\n    • 孫\n• ☐ 未完了\n• ☑ 完了\n1. 最初\n2. 次\n> 引用 本文\n\n引用\n次の行\n\n通常\t本文\n\n"
    );
    assert_eq!(raw, original);
}

#[test]
fn backlog_code_escapes_and_unknown_syntax_preserve_literals_and_malformed_content() {
    let raw = "{code:java}\n* ''literal'' &br; BLG-104\n- [x] code\n{/code}\n{code}''inline'' BLG-104{/code}\n\\* 見出しでない\n\\''literal\\'' \\&br;\n#unknown(''原文'' BLG-104)\n&color(red){''本文''};\n<div>''HTML'' BLG-104</div>\n{quote}未完了\n{/quote}\n''未完了\n{code}\n* 未閉鎖も保持\n";
    assert_eq!(body::display(raw, "backlog", &[]), raw);
    for raw in [
        "{quote}未閉鎖",
        "{/quote}",
        "''未閉鎖",
        "[[未閉鎖",
        "{unknown}''保持''{/unknown}",
    ] {
        assert_eq!(body::display(raw, "backlog", &[]), raw, "{raw:?}");
    }
}

#[test]
fn backlog_inline_quotes_keep_unknown_macros_and_html_as_whole_raw_lines() {
    let origin = reqwest::Url::parse("https://space.test/").unwrap();
    for raw in [
        "{quote}#unknown(''raw'' BLG-1){/quote}",
        "{quote}<b>''raw'' BLG-1</b>{/quote}\n",
        "{quote}&color(red){''raw'' BLG-1};{/quote}\n",
        "{quote}''本文''{/quote} #unknown(''raw'' BLG-1)\n",
    ] {
        assert_eq!(body::display(raw, "backlog", &[]), raw, "{raw:?}");
        assert_eq!(
            body::display_with_origin(raw, "backlog", &origin, &[]),
            raw,
            "{raw:?}"
        );
    }
    assert_eq!(
        body::display_with_origin("{quote}''本文'' BLG-1{/quote}\n", "backlog", &origin, &[]),
        "本文 BLG-1 (https://space.test/view/BLG-1)\n"
    );
}

#[test]
fn backlog_unclosed_inline_markers_keep_the_remainder_without_reinterpreting_it() {
    let origin = reqwest::Url::parse("https://space.test/").unwrap();
    for raw in [
        "'''literal''",
        "'''literal'' BLG-104 ''raw''\n",
        "''literal %%raw%% BLG-104\n",
        "%%literal ''raw'' BLG-104\n",
    ] {
        assert_eq!(body::display(raw, "backlog", &[]), raw, "{raw:?}");
        assert_eq!(
            body::display_with_origin(raw, "backlog", &origin, &[]),
            raw,
            "{raw:?}"
        );
    }
    assert_eq!(
        body::display_with_origin("''本文'' '''literal'' BLG-104\n", "backlog", &origin, &[]),
        "本文 '''literal'' BLG-104\n"
    );
}

#[test]
fn backlog_normal_tables_render_cells_but_complex_tables_keep_all_source() {
    let raw = "| 名前 | 値 |h\n|~項目|''日本語''|\n|é 👩‍💻|値&br;次|\n\n|a|>|\n|~複雑|^|\n|A\\|B|C|\n|不完全\n";
    assert_eq!(
        body::display(raw, "backlog", &[]),
        "名前\t値\n項目\t日本語\né 👩‍💻\t値\n次\n\n|a|>|\n|~複雑|^|\n|A\\|B|C|\n|不完全\n"
    );
}

#[test]
fn backlog_links_use_configured_origin_and_preserve_reference_information() {
    let origin = reqwest::Url::parse("https://space.example:8443/").unwrap();
    let raw = "BLG-104 [[BLG-87]] [[P_2-123456789012345678901234567890]]\n[[https://example.test/a]] [[表示>https://example.test/a]] [[名:https://example.test/b]]\n[[Wiki ページ]] [[Git:repo:abc123]] #attach(sample.zip:11) #image(11) #thumbnail(11) #rev(11) #contents\n\\BLG-104 \\[[BLG-87]]\n{code}\nBLG-104 [[BLG-87]]\n{/code}\n[[../BLG-1]] [[BLG-1?x]] [[https://evil.test/BLG-1]]\n../BLG-1 BLG-0 BLG-1?x https://evil.test/BLG-1 日本語BLG-1\n";
    assert_eq!(
        body::display_with_origin(raw, "backlog", &origin, &[]),
        "BLG-104 (https://space.example:8443/view/BLG-104) BLG-87 (https://space.example:8443/view/BLG-87) P_2-123456789012345678901234567890 (https://space.example:8443/view/P_2-123456789012345678901234567890)\nhttps://example.test/a 表示 (https://example.test/a) 名 (https://example.test/b)\n[[Wiki ページ]] [[Git:repo:abc123]] #attach(sample.zip:11) #image(11) #thumbnail(11) #rev(11) #contents\n\\BLG-104 \\[[BLG-87]]\n{code}\nBLG-104 [[BLG-87]]\n{/code}\n[[../BLG-1]] [[BLG-1?x]] https://evil.test/BLG-1\n../BLG-1 BLG-0 BLG-1?x https://evil.test/BLG-1 日本語BLG-1\n"
    );
    assert_eq!(
        body::display("BLG-104 [[BLG-87]]", "backlog", &[]),
        "BLG-104 [[BLG-87]]"
    );
    for invalid in [
        "http://space.test/",
        "https://user:pass@space.test/",
        "https://space.test/path",
        "https://space.test/?key=secret",
        "https://space.test/#fragment",
        "file:///tmp/",
    ] {
        let origin = reqwest::Url::parse(invalid).unwrap();
        assert_eq!(
            body::display_with_origin("BLG-104 [[BLG-87]]", "backlog", &origin, &[]),
            "BLG-104 [[BLG-87]]"
        );
    }
}

#[test]
fn backlog_safety_runs_after_compound_inline_changes_and_keeps_source_unchanged() {
    let raw = "* test''-secret'' é 👩‍💻\nA&br;B ''test''-secret\n|''test''-secret|\n[[test''-secret''>https://example.test/test-secret]]\nBLG-104\n{code}\ntest-secret\t\u{1b}]52;c;bad\u{7}\r\u{9b}\n{/code}\n<b>&#27; test-secret</b>\n".to_owned();
    let original = raw.clone();
    let origin = reqwest::Url::parse("https://space.test/").unwrap();
    let shown = body::display_with_origin(
        &raw,
        "backlog",
        &origin,
        &["test-secret", "https://space.test/view/BLG-104"],
    );
    assert_eq!(
        shown,
        "[REDACTED] é 👩‍💻\nA\nB [REDACTED]\n[REDACTED]\n[REDACTED] (https://example.test/[REDACTED])\nBLG-104 ([REDACTED])\n{code}\n[REDACTED]\t\\u{1b}]52;c;bad\\u{7}\\u{d}\\u{9b}\n{/code}\n<b>&#27; [REDACTED]</b>\n"
    );
    assert_eq!(raw, original);
    assert_eq!(
        body::display("''\u{1b}''", "backlog", &["\\u{1b}"]),
        "[REDACTED]"
    );
}

#[test]
fn backlog_escaped_block_openers_do_not_change_later_lines_and_unknown_lines_stay_raw() {
    let raw = "\\{code}\n* 見出し\n\\{code:java}\n- 項目\n* #unknown(''保持'')\n- &color(red){''保持''};\n* <div>''保持''</div>\n\\{quote}\n* 次の見出し\n{/quote}\n";
    assert_eq!(
        body::display(raw, "backlog", &[]),
        "\\{code}\n見出し\n\\{code:java}\n• 項目\n* #unknown(''保持'')\n- &color(red){''保持''};\n* <div>''保持''</div>\n\\{quote}\n次の見出し\n{/quote}\n"
    );
}

#[test]
fn backlog_code_closing_and_reopening_on_one_line_protects_every_span() {
    let origin = reqwest::Url::parse("https://space.test/").unwrap();
    for code in [
        "{code}one{/code} {code}\n* ''literal'' BLG-104\n{/code}\n",
        "{code}\none\n{/code} {code:java}\n* ''literal'' BLG-104\n{/code}\n",
        "{code}one{/code} {code}two{/code} {code}\n* ''literal'' BLG-104\n{/code}\n",
    ] {
        let raw = format!("{code}* ''本文'' BLG-104\n");
        assert_eq!(
            body::display_with_origin(&raw, "backlog", &origin, &[]),
            format!("{code}本文 BLG-104 (https://space.test/view/BLG-104)\n"),
            "{raw:?}"
        );
        assert_eq!(
            body::display(&raw, "backlog", &[]),
            format!("{code}本文 BLG-104\n"),
            "{raw:?}"
        );
    }
}

#[test]
fn backlog_raw_tables_keep_code_protection_for_following_lines() {
    let origin = reqwest::Url::parse("https://space.test/").unwrap();
    for table_and_code in [
        "|a|b|\n|{code}|x|\n* ''literal'' BLG-104\n{/code}\n",
        "|a|b|\n|{code}one{/code} {code:java}|x|\n* ''literal'' BLG-104\n{/code}\n",
        "|a|b|\n|{code}|x|\n|{/code} {code}|y|\n* ''literal'' BLG-104\n{/code}\n",
    ] {
        let raw = format!("{table_and_code}* ''本文'' BLG-104\n");
        assert_eq!(
            body::display_with_origin(&raw, "backlog", &origin, &[]),
            format!("{table_and_code}本文 BLG-104 (https://space.test/view/BLG-104)\n"),
            "{raw:?}"
        );
        assert_eq!(
            body::display(&raw, "backlog", &[]),
            format!("{table_and_code}本文 BLG-104\n"),
            "{raw:?}"
        );
    }
}

#[test]
fn backlog_escaped_code_closers_do_not_end_literal_protection() {
    let raw = "{code}\n\\{/code}\n* ''code'' BLG-104\n{/code}\n* 本文\n";
    let origin = reqwest::Url::parse("https://space.test/").unwrap();
    assert_eq!(
        body::display_with_origin(raw, "backlog", &origin, &[]),
        "{code}\n\\{/code}\n* ''code'' BLG-104\n{/code}\n本文\n"
    );
}
