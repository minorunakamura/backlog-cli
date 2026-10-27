use backlog_cli::body;

#[test]
fn display_keeps_raw_body_for_unsupported_rules_without_changing_the_source() {
    let raw = " # 見出し é 👩‍💻\n<b>&#27;</b> [[unknown]]\n```\n*code*\n```\tsecret\u{1b}]52;c;bad\u{7}\r\u{9b}".to_owned();
    let original = raw.clone();
    for rule in ["backlog", "future-rule", ""] {
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
