use backlog_cli::body;

#[test]
fn display_keeps_raw_body_for_every_rule_without_changing_the_source() {
    let raw = " # 見出し é 👩‍💻\n<b>&#27;</b> [[unknown]]\n```\n*code*\n```\tsecret\u{1b}]52;c;bad\u{7}\r\u{9b}".to_owned();
    let original = raw.clone();
    for rule in ["markdown", "backlog", "future-rule", ""] {
        assert_eq!(
            body::display(&raw, rule, &["secret"]),
            " # 見出し é 👩‍💻\n<b>&#27;</b> [[unknown]]\n```\n*code*\n```\t[REDACTED]\\u{1b}]52;c;bad\\u{7}\\u{d}\\u{9b}"
        );
        assert_eq!(raw, original);
    }
    assert_eq!(body::display("", "markdown", &[]), "");
}
