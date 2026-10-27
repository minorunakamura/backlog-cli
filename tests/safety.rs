use backlog_cli::safety;
use serde_json::{Value, json};

#[test]
fn safe_display_protects_every_json_scalar_and_preserves_originals() {
    let original: Value = serde_json::from_str(r#"{"token-key":["日本語 é 👩‍💻\n\t\u001b]52;c;token\u0007\u009b31m",123456789012345678901234567890,true,null],"big":999999999999999999999999999999}"#).unwrap();
    let before = original.clone();
    let safe = safety::json(&original, &["token", "1234567890", "true", "null"]).unwrap();
    assert_eq!(safe["[REDACTED]-key"][1], "[REDACTED][REDACTED][REDACTED]");
    assert_eq!(safe["[REDACTED]-key"][2], "[REDACTED]");
    assert_eq!(safe["[REDACTED]-key"][3], "[REDACTED]");
    assert!(
        safe["[REDACTED]-key"][0]
            .as_str()
            .unwrap()
            .starts_with("日本語 é 👩‍💻\n\t")
    );
    assert!(!safe.to_string().contains("token"));
    assert!(
        !safe["[REDACTED]-key"][0]
            .as_str()
            .unwrap()
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\t')
    );
    assert!(safe.to_string().contains("999999999999999999999999999999"));
    assert_eq!(original, before);
}

#[test]
fn safe_display_reports_redacted_key_collisions_without_losing_fields() {
    let error = safety::json(&json!({"token":1,"[REDACTED]":2}), &["token"]).unwrap_err();
    assert_eq!(error.kind, "output");
}

#[test]
fn safe_display_cannot_reintroduce_a_secret_through_control_escaping_or_the_marker() {
    assert_eq!(safety::text("\u{1b}", &[r"\u{1b}"]), "[REDACTED]");
    assert_eq!(safety::text("REDACTED", &["REDACTED"]), "");
    assert_eq!(
        safety::text("<b>&#27; text</b>\r\u{7f}\u{9b}", &[]),
        r"<b>&#27; text</b>\u{d}\u{7f}\u{9b}"
    );
}
