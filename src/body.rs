use crate::safety;
use pulldown_cmark::{Event, Options, Parser, Tag};
use std::ops::Range;

/// Return a plain display copy; API and editor text remain untouched.
pub fn display(raw: &str, text_formatting_rule: &str, secrets: &[&str]) -> String {
    let rendered = if text_formatting_rule == "markdown" {
        markdown(raw)
    } else {
        raw.to_owned()
    };
    safety::text(&rendered, secrets)
}

// Preserve source gaps so HTML, reference definitions, extensions and whitespace are not dropped.
fn markdown(raw: &str) -> String {
    let events: Vec<_> = Parser::new_ext(raw, Options::ENABLE_TASKLISTS | Options::ENABLE_TABLES)
        .into_offset_iter()
        .collect();
    let mut edits = Vec::new();
    let mut skip_until = 0;
    for (index, (event, range)) in events.iter().enumerate() {
        if index < skip_until {
            continue;
        }
        match event {
            Event::Start(Tag::CodeBlock(_)) => skip_until = tag_end(&events, index) + 1,
            Event::Start(Tag::Table(_)) => {
                let end = tag_end(&events, index);
                if let Some(table_edits) = table_edits(raw, range.clone(), &events[index + 1..end])
                {
                    edits.extend(table_edits);
                } else {
                    skip_until = end + 1;
                }
            }
            Event::Text(text) if text.as_ref() != &raw[range.clone()] => {
                edits.push((range.clone(), text.to_string()));
            }
            Event::Start(Tag::Emphasis | Tag::Strong) => {
                let width = if matches!(event, Event::Start(Tag::Strong)) {
                    2
                } else {
                    1
                };
                edits.push((range.start..range.start + width, String::new()));
                edits.push((range.end - width..range.end, String::new()));
            }
            Event::Start(Tag::Heading { .. }) => {
                let source = &raw[range.clone()];
                let first = source.lines().next().unwrap_or("");
                let hashes = first.len() - first.trim_start_matches('#').len();
                if hashes == 0
                    || hashes > 6
                    || !first[hashes..].starts_with([' ', '\t']) && hashes != first.len()
                {
                    let underline = source
                        .trim_end_matches('\n')
                        .rfind('\n')
                        .map_or(0, |i| i + 1);
                    edits.push((range.start + underline..range.end, String::new()));
                } else {
                    let prefix =
                        first.len() - first[hashes..].trim_start_matches([' ', '\t']).len();
                    edits.push((range.start..range.start + prefix, String::new()));
                    let trimmed = first.trim_end_matches([' ', '\t']);
                    let before_closing = trimmed.trim_end_matches('#');
                    if before_closing.len() < trimmed.len() && before_closing.ends_with([' ', '\t'])
                    {
                        let closing = before_closing
                            .trim_end_matches([' ', '\t'])
                            .len()
                            .max(prefix);
                        edits.push((
                            range.start + closing..range.start + first.len(),
                            String::new(),
                        ));
                    }
                }
            }
            Event::Start(Tag::Item) => {
                let source = &raw[range.clone()];
                let trimmed = source.trim_start_matches([' ', '\t']);
                if trimmed.starts_with(['-', '+', '*']) {
                    let marker = range.start + source.len() - trimmed.len();
                    edits.push((marker..marker + 1, "•".into()));
                }
            }
            Event::TaskListMarker(checked) => {
                edits.push((range.clone(), if *checked { "☑" } else { "☐" }.into()));
            }
            Event::Start(
                Tag::Link {
                    dest_url, title, ..
                }
                | Tag::Image {
                    dest_url, title, ..
                },
            ) => {
                let end = tag_end(&events, index);
                let children = &events[index + 1..end];
                let source = &raw[range.clone()];
                let prefix = if source.starts_with("![") { 2 } else { 1 };
                let label_start = range.start + prefix;
                let label_end = children.last().map_or(label_start, |(_, span)| span.end);
                edits.push((range.start..label_start, String::new()));
                let label = &raw[label_start..label_end];
                let mut suffix = if label == dest_url.as_ref() {
                    String::new()
                } else {
                    format!(" ({dest_url})")
                };
                if !title.is_empty() {
                    suffix.push_str(&format!(" \"{title}\""));
                }
                edits.push((label_end..range.end, suffix));
            }
            Event::Code(_) => {
                let source = &raw[range.clone()];
                let width = source.len() - source.trim_start_matches('`').len();
                edits.push((range.start..range.start + width, String::new()));
                edits.push((range.end - width..range.end, String::new()));
            }
            _ => {}
        }
    }
    apply_edits(raw, edits)
}

fn table_edits(
    raw: &str,
    table: Range<usize>,
    events: &[(Event<'_>, Range<usize>)],
) -> Option<Vec<(Range<usize>, String)>> {
    let mut edits = Vec::new();
    let mut head_end = table.start;
    let mut separator_end = table.end;
    for (index, (event, row)) in events.iter().enumerate() {
        if !matches!(event, Event::Start(Tag::TableHead | Tag::TableRow)) {
            continue;
        }
        if matches!(event, Event::Start(Tag::TableHead)) {
            head_end = row.end;
        } else {
            separator_end = separator_end.min(row.start);
        }
        let end = tag_end(events, index);
        let mut cursor = row.start;
        let mut first = true;
        for (event, cell) in &events[index + 1..end] {
            if !matches!(event, Event::Start(Tag::TableCell)) {
                continue;
            }
            if cell.start < cursor || cell.end > row.end {
                return None;
            }
            if !raw[cursor..cell.start]
                .chars()
                .all(|c| matches!(c, '|' | ' ' | '\t' | '\n'))
            {
                return None;
            }
            let source = &raw[cell.clone()];
            let leading = source.len() - source.trim_start_matches(' ').len();
            let trailing = source.trim_end_matches(' ').len().max(leading);
            edits.push((
                cursor..cell.start + leading,
                if first { "" } else { "\t" }.into(),
            ));
            edits.push((cell.start + trailing..cell.end, String::new()));
            cursor = cell.end;
            first = false;
        }
        if first
            || !raw[cursor..row.end]
                .chars()
                .all(|c| matches!(c, '|' | ' ' | '\t' | '\n'))
        {
            return None;
        }
        edits.push((
            cursor..row.end,
            if raw[row.clone()].ends_with('\n') {
                "\n"
            } else {
                ""
            }
            .into(),
        ));
    }
    if !raw[head_end..separator_end]
        .chars()
        .all(|c| matches!(c, '|' | ':' | '-' | ' ' | '\t' | '\n'))
    {
        return None;
    }
    edits.push((head_end..separator_end, String::new()));
    Some(edits)
}

fn tag_end(events: &[(Event<'_>, Range<usize>)], start: usize) -> usize {
    let mut depth = 0;
    for (index, (event, _)) in events.iter().enumerate().skip(start) {
        match event {
            Event::Start(_) => depth += 1,
            Event::End(_) => {
                depth -= 1;
                if depth == 0 {
                    return index;
                }
            }
            _ => {}
        }
    }
    unreachable!("pulldown-cmark tags are balanced")
}

fn apply_edits(raw: &str, mut edits: Vec<(Range<usize>, String)>) -> String {
    edits.sort_by_key(|(range, _)| range.start);
    let mut output = String::new();
    let mut cursor = 0;
    for (span, replacement) in edits {
        if span.start < cursor || span.end > raw.len() {
            return raw.to_owned();
        }
        output.push_str(&raw[cursor..span.start]);
        output.push_str(&replacement);
        cursor = span.end;
    }
    output.push_str(&raw[cursor..]);
    output
}
