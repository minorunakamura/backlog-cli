use crate::Error;
use serde_json::Value;

/// Return a display copy; never change the API data used by callers.
pub fn text(value: &str, secrets: &[&str]) -> String {
    let contains_secret = |s: &str| {
        secrets
            .iter()
            .any(|secret| !secret.is_empty() && s.contains(secret))
    };
    let marker = if contains_secret("[REDACTED]") {
        ""
    } else {
        "[REDACTED]"
    };
    let mut safe = value.to_owned();
    for secret in secrets.iter().filter(|s| !s.is_empty()) {
        safe = safe.replace(secret, marker);
    }
    let safe: String = safe
        .chars()
        .flat_map(|c| {
            if c.is_control() && c != '\n' && c != '\t' {
                c.escape_unicode().collect::<Vec<_>>()
            } else {
                vec![c]
            }
        })
        .collect();
    if contains_secret(&safe) {
        marker.into()
    } else {
        safe
    }
}

pub fn json(value: &Value, secrets: &[&str]) -> Result<Value, Error> {
    Ok(match value {
        Value::String(s) => Value::String(text(s, secrets)),
        Value::Number(_) | Value::Bool(_) | Value::Null
            if secrets
                .iter()
                .any(|s| !s.is_empty() && value.to_string().contains(s)) =>
        {
            Value::String(text(&value.to_string(), secrets))
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|v| json(v, secrets))
                .collect::<Result<_, _>>()?,
        ),
        Value::Object(object) => {
            let mut safe = serde_json::Map::new();
            for (key, value) in object {
                if safe
                    .insert(text(key, secrets), json(value, secrets)?)
                    .is_some()
                {
                    return Err(Error::new(
                        "output",
                        "Safe display would produce duplicate object keys",
                    ));
                }
            }
            Value::Object(safe)
        }
        _ => value.clone(),
    })
}
