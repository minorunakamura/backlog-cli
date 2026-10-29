//! Shared safe display of validated comment responses.
use crate::{Error, body, config::Config, safety};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
};
use serde_json::Value;
use similar::{ChangeTag, DiffTag, TextDiff};
use std::time::{Duration, Instant};

const DELETION_BACKGROUND: Color = Color::Rgb(128, 0, 0);

/// Format a comment validated by `Service::comment_list` or `Service::comment_view`.
pub fn display(item: &Value, rule: &str, config: &Config) -> Result<String, Error> {
    Ok(format!(
        "{}{}\n",
        metadata(item, "\t", config),
        content(item, rule, "\t", config)?
    ))
}

/// Safe TUI display copies, prepared in the existing acquisition task.
pub struct Preview {
    pub author: String,
    pub created: String,
    pub text: Text<'static>,
    pub no_color_text: Text<'static>,
}
pub fn preview(item: &Value, rule: &str, config: &Config) -> Result<Preview, Error> {
    let secrets = &[config.api_key.as_str()];
    let mut spans = Vec::new();
    if let Some(content) = item["content"].as_str() {
        spans.push(Span::raw(body::display_with_origin(
            content,
            rule,
            &config.origin,
            secrets,
        )));
    }
    if let Some(changes) = item["changeLog"].as_array() {
        for change in changes {
            if !spans.is_empty() {
                spans.push(Span::raw("\n\n"));
            }
            let field = change["field"].as_str().expect("validated history field");
            if field == "description"
                && let (Some(old), Some(new)) = (
                    change["originalValue"].as_str(),
                    change["newValue"].as_str(),
                )
            {
                description(&mut spans, old, new, secrets);
                continue;
            }
            let mut values = Vec::new();
            for value_field in ["originalValue", "newValue"] {
                let safe = safety::json(&change[value_field], secrets)?;
                values.push(if change[value_field].is_null() {
                    "未設定".into()
                } else if field == "description" {
                    safe.to_string()
                } else {
                    safe.as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| safe.to_string())
                });
            }
            spans.push(Span::raw(format!(
                "{}: {} -> {}",
                if field == "description" {
                    "詳細"
                } else {
                    label(field)
                },
                values[0],
                values[1]
            )));
        }
    }
    let text = document(spans.clone(), secrets);
    let plain = spans
        .into_iter()
        .map(|span| {
            let content = if span.style.bg == Some(DELETION_BACKGROUND) {
                format!("[-{}-]", span.content)
            } else if span.style.bg == Some(Color::Green) {
                format!("{{+{}+}}", span.content)
            } else {
                span.content.into_owned()
            };
            Span::raw(content)
        })
        .collect();
    Ok(Preview {
        author: safety::text(
            item["createdUser"]["name"]
                .as_str()
                .expect("validated author"),
            secrets,
        ),
        created: crate::tui_datetime::display(
            item["created"].as_str().expect("validated date"),
            secrets,
        ),
        text,
        no_color_text: document(plain, secrets),
    })
}
fn label(field: &str) -> &str {
    match field {
        "summary" => "TITLE",
        "description" => "BODY",
        "issueType" => "TYPE",
        "priority" => "PRIORITY",
        "status" => "STATUS",
        "assigner" | "assignee" => "ASSIGNEE",
        "resolution" => "RESOLUTION",
        "startDate" => "START DATE",
        "limitDate" | "dueDate" => "DUE DATE",
        "milestone" => "MILESTONES",
        _ => field,
    }
}
fn description(spans: &mut Vec<Span<'static>>, old: &str, new: &str, secrets: &[&str]) {
    if old.chars().count() + new.chars().count() > 10_000 {
        spans.push(Span::raw("詳細\n"));
        spans.push(Span::styled(
            "変更内容が10,000文字を超えるため、差分を表示できません。",
            Style::default().fg(Color::Yellow),
        ));
        return;
    }
    let heading = if old == new {
        "詳細（変更なし）"
    } else if new.is_empty() {
        "詳細（空へ変更）"
    } else if old.is_empty() {
        "詳細（空から追加）"
    } else {
        "詳細"
    };
    spans.push(Span::raw(format!("{heading}\n")));
    if old.is_empty() && new.is_empty() {
        spans.push(Span::raw("（空）"));
        return;
    }
    let old = safety::text(old, secrets);
    let new = safety::text(new, secrets);
    let deadline = Instant::now() + Duration::from_millis(50);
    let diff = TextDiff::configure()
        .deadline(deadline)
        .diff_lines(&old, &new);
    let mut changes: Vec<(ChangeTag, String)> = Vec::new();
    for op in diff.ops() {
        let (tag, old_range, new_range) = op.as_tag_tuple();
        if tag == DiffTag::Replace {
            let old_lines = &diff.old_slices()[old_range];
            let new_lines = &diff.new_slices()[new_range];
            for index in 0..old_lines.len().max(new_lines.len()) {
                let old = old_lines.get(index).copied().unwrap_or("");
                let new = new_lines.get(index).copied().unwrap_or("");
                // Both comparisons share one deadline; coarse approximations still retain all text.
                for change in TextDiff::configure()
                    .deadline(deadline)
                    .diff_graphemes(old, new)
                    .iter_all_changes()
                {
                    push_change(&mut changes, change.tag(), change.value());
                }
            }
        } else {
            for change in diff.iter_changes(op) {
                push_change(&mut changes, change.tag(), change.value());
            }
        }
    }
    for (tag, value) in changes {
        let style = match tag {
            ChangeTag::Equal => Style::default(),
            ChangeTag::Delete => Style::default()
                .fg(Color::Rgb(255, 255, 255))
                .bg(DELETION_BACKGROUND)
                .add_modifier(Modifier::CROSSED_OUT),
            ChangeTag::Insert => Style::default().fg(Color::Black).bg(Color::Green),
        };
        spans.push(Span::styled(value, style));
    }
}
fn push_change(changes: &mut Vec<(ChangeTag, String)>, tag: ChangeTag, value: &str) {
    if let Some((_, last_value)) = changes.last_mut().filter(|(last_tag, _)| *last_tag == tag) {
        last_value.push_str(value);
    } else {
        changes.push((tag, value.into()));
    }
}
/// Recheck the complete projection, not individual spans, before retaining any decoration.
fn document(mut spans: Vec<Span<'static>>, secrets: &[&str]) -> Text<'static> {
    for span in &mut spans {
        if matches!(span.style.bg, Some(DELETION_BACKGROUND | Color::Green)) {
            // A changed newline needs a styled cell as well as its original line break.
            span.content = span.content.replace('\n', "↵\n").into();
        }
    }
    let projection: String = spans.iter().map(|span| span.content.as_ref()).collect();
    let safe = safety::text(&projection, secrets);
    let spans = if safe == projection {
        spans
    } else {
        vec![Span::raw(safe)]
    };
    let mut lines = vec![Line::default()];
    for span in spans {
        for (index, part) in span.content.split('\n').enumerate() {
            if index > 0 {
                lines.push(Line::default());
            }
            lines
                .last_mut()
                .unwrap()
                .spans
                .push(Span::styled(part.to_owned(), span.style));
        }
    }
    crate::tui_text::display(Text::from(lines), secrets)
}
fn metadata(item: &Value, separator: &str, config: &Config) -> String {
    let secrets = &[config.api_key.as_str()];
    let mut output = format!(
        "ID{separator}{}\nTYPE{separator}{}\n",
        safety::text(&item["id"].to_string(), secrets),
        if item["content"].is_null() {
            "CHANGE HISTORY"
        } else {
            "COMMENT"
        }
    );
    for (label, value) in [
        ("AUTHOR", &item["createdUser"]["name"]),
        ("CREATED", &item["created"]),
        ("UPDATED", &item["updated"]),
    ] {
        output.push_str(&format!(
            "{label}{separator}{}\n",
            safety::text(value.as_str().expect("validated comment field"), secrets)
        ));
    }
    output
}
fn content(item: &Value, rule: &str, separator: &str, config: &Config) -> Result<String, Error> {
    let secrets = &[config.api_key.as_str()];
    let mut output = String::new();
    if let Some(content) = item["content"].as_str() {
        output.push_str("BODY\n");
        output.push_str(&body::display_with_origin(
            content,
            rule,
            &config.origin,
            secrets,
        ));
        output.push('\n');
    }
    if let Some(changes) = item["changeLog"]
        .as_array()
        .filter(|changes| !changes.is_empty())
    {
        output.push_str("CHANGES\n");
        for change in changes {
            let field = change["field"].as_str().expect("validated history field");
            let label = label(field);
            let mut values = Vec::new();
            for field in ["originalValue", "newValue"] {
                let safe = safety::json(&change[field], secrets)?;
                values.push(
                    safe.as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| safe.to_string()),
                );
            }
            output.push_str(&format!(
                "{}{separator}{} -> {}\n",
                safety::text(label, secrets),
                values[0],
                values[1]
            ));
        }
    }
    Ok(output)
}
