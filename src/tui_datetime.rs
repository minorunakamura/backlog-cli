//! Fixed-offset display only; API and CLI values stay untouched.
use crate::safety;
use time::{
    OffsetDateTime, UtcOffset,
    format_description::{self, well_known::Rfc3339},
};

pub(crate) fn display(value: &str, secrets: &[&str]) -> String {
    let converted = OffsetDateTime::parse(value, &Rfc3339)
        .ok()
        .and_then(|date| date.checked_to_offset(UtcOffset::from_hms(9, 0, 0).unwrap()))
        .and_then(|date| {
            date.format(
                &format_description::parse_borrowed::<2>(
                    "[year]/[month]/[day] [hour]:[minute]:[second]",
                )
                .unwrap(),
            )
            .ok()
        });
    safety::text(
        &converted.unwrap_or_else(|| format!("{value}（変換不可）")),
        secrets,
    )
}
