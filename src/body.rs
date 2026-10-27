use crate::safety;

/// All formatting rules currently fall back to raw text; never interpret HTML or entities.
pub fn display(raw: &str, _text_formatting_rule: &str, secrets: &[&str]) -> String {
    safety::text(raw, secrets)
}
