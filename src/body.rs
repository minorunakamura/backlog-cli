use crate::safety;
use pulldown_cmark::{Event, Options, Parser, Tag};
use std::ops::Range;

/// Return a plain display copy; API and editor text remain untouched.
pub fn display(raw: &str, text_formatting_rule: &str, secrets: &[&str]) -> String {
    render(raw, text_formatting_rule, None, secrets)
}

/// Display issue links using an HTTPS origin; invalid origins leave issue references unchanged.
pub fn display_with_origin(
    raw: &str,
    text_formatting_rule: &str,
    origin: &reqwest::Url,
    secrets: &[&str],
) -> String {
    let origin = crate::config::origin(origin.as_str()).ok();
    render(raw, text_formatting_rule, origin.as_ref(), secrets)
}

fn render(raw: &str, rule: &str, origin: Option<&reqwest::Url>, secrets: &[&str]) -> String {
    let rendered = match rule {
        "markdown" => markdown(raw),
        "backlog" => backlog(raw, origin),
        _ => raw.to_owned(),
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

fn backlog(raw: &str, origin: Option<&reqwest::Url>) -> String {
    let mut output = String::new();
    let mut number = 0;
    let mut offset = 0;
    let mut quote_end = None;
    let mut in_code = false;
    let mut table_end = 0;
    for line in raw.split_inclusive('\n') {
        let start = offset;
        offset += line.len();
        // Raw table rows can open code that continues beyond the table.
        let mut protect_code = in_code;
        let mut cursor = 0;
        loop {
            let rest = &line[cursor..];
            let marker = if in_code {
                backlog_unescaped_find(rest, "{/code}")
            } else {
                ["{code}", "{code:"]
                    .into_iter()
                    .filter_map(|marker| backlog_unescaped_find(rest, marker))
                    .min()
            };
            let Some(marker) = marker else { break };
            cursor += marker + if in_code { 7 } else { 6 };
            in_code = !in_code;
            protect_code = true;
        }
        if start < table_end {
            continue;
        }
        if protect_code {
            output.push_str(line);
            continue;
        }
        if line.starts_with('|') {
            let table = &raw[start
                ..start
                    + raw[start..]
                        .split_inclusive('\n')
                        .take_while(|row| row.starts_with('|'))
                        .map(str::len)
                        .sum::<usize>()];
            table_end = start + table.len();
            output.push_str(&backlog_table(table, origin).unwrap_or_else(|| table.to_owned()));
            continue;
        }
        if line.trim_end_matches('\n') == "{quote}"
            && let Some(end) = backlog_unescaped_find(&raw[offset..], "{/quote}")
        {
            quote_end = Some(offset + end);
            output.push_str(&line[7..]);
            continue;
        }
        if quote_end == Some(start) {
            output.push_str(&line[8..]);
            quote_end = None;
            continue;
        }
        // Unknown macros and HTML stay verbatim, including their surrounding syntax.
        if line.contains(['#', '{', '<'])
            && !(line.starts_with("{quote}")
                && line.contains("{/quote}")
                && !line
                    .replace("{quote}", "")
                    .replace("{/quote}", "")
                    .contains(['#', '{', '<']))
        {
            output.push_str(line);
            number = 0;
            continue;
        }
        let mut text = line;
        let heading = line.len() - line.trim_start_matches('*').len();
        let bullet = line.len() - line.trim_start_matches('-').len();
        if (1..=3).contains(&heading) {
            text = line[heading..].trim_start_matches([' ', '\t']);
        } else if (1..=3).contains(&bullet) {
            output.push_str(&"  ".repeat(bullet - 1));
            output.push_str("• ");
            text = line[bullet..].trim_start_matches([' ', '\t']);
            if let Some(rest) = text.strip_prefix("[ ] ") {
                output.push_str("☐ ");
                text = rest;
            } else if let Some(rest) = text.strip_prefix("[x] ") {
                output.push_str("☑ ");
                text = rest;
            }
        } else if let Some(rest) = line.strip_prefix("+ ") {
            number += 1;
            output.push_str(&format!("{number}. "));
            text = rest;
        }
        if !line.starts_with("+ ") {
            number = 0;
        }
        output.push_str(&backlog_inline(text, origin));
    }
    output
}

fn backlog_inline(raw: &str, origin: Option<&reqwest::Url>) -> String {
    let mut output = String::new();
    let mut cursor = 0;
    while cursor < raw.len() {
        let rest = &raw[cursor..];
        if let Some(escaped) = rest.strip_prefix('\\') {
            if escaped.starts_with("[[") {
                let end = escaped.find("]]").map_or(rest.len(), |i| i + 3);
                output.push_str(&rest[..end]);
                cursor += end;
                continue;
            }
            let width = ["'''", "''", "%%", "&br;", "[["]
                .into_iter()
                .find(|m| escaped.starts_with(m))
                .map_or_else(
                    || escaped.chars().next().map_or(0, char::len_utf8),
                    str::len,
                );
            output.push_str(&rest[..1 + width]);
            cursor += 1 + width;
            continue;
        }
        if let Some(link) = rest.strip_prefix("[[") {
            let Some(end) = link.find("]]") else {
                output.push_str(rest);
                break;
            };
            let link = &link[..end];
            if let Some(origin) = origin.filter(|_| crate::service::issue_key(link)) {
                output.push_str(&format!("{link} ({})", crate::api::issue_url(origin, link)));
            } else if let Some((label, url)) = backlog_url_link(link) {
                if label == url {
                    output.push_str(url);
                } else {
                    output.push_str(&format!("{} ({url})", backlog_inline(label, origin)));
                }
            } else {
                output.push_str(&rest[..end + 4]);
            }
            cursor += end + 4;
            continue;
        }
        if rest.starts_with("&br;") {
            output.push('\n');
            cursor += 4;
            continue;
        }
        if rest.starts_with("{quote}")
            && let Some(end) = backlog_unescaped_find(&rest[7..], "{/quote}")
        {
            output.push_str(&backlog_inline(&rest[7..7 + end], origin));
            cursor += 15 + end;
            continue;
        }
        let marker = ["'''", "''", "%%"]
            .into_iter()
            .find(|m| rest.starts_with(m));
        if let Some(marker) = marker {
            let Some(end) = backlog_unescaped_find(&rest[marker.len()..], marker) else {
                output.push_str(rest);
                break;
            };
            output.push_str(&backlog_inline(
                &rest[marker.len()..marker.len() + end],
                origin,
            ));
            cursor += marker.len() * 2 + end;
            continue;
        }
        if let Some(origin) = origin
            && rest.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_')
            && (cursor == 0 || raw[..cursor].ends_with(backlog_link_boundary))
        {
            let end = rest.find(backlog_link_boundary).unwrap_or(rest.len());
            let key = &rest[..end];
            if crate::service::issue_key(key) {
                output.push_str(&format!("{key} ({})", crate::api::issue_url(origin, key)));
                cursor += end;
                continue;
            }
        }
        let ch = rest.chars().next().expect("nonempty remainder");
        output.push(ch);
        cursor += ch.len_utf8();
    }
    output
}

fn backlog_table(raw: &str, origin: Option<&reqwest::Url>) -> Option<String> {
    let mut output = String::new();
    for line in raw.split_inclusive('\n') {
        let row = line.trim_end_matches('\n');
        let row = row.strip_suffix('h').unwrap_or(row);
        let cells = row.strip_prefix('|')?.strip_suffix('|')?;
        if cells.contains(['\\', '{', '<', '#']) {
            return None;
        }
        for (index, cell) in cells.split('|').enumerate() {
            let cell = cell.trim_matches([' ', '\t']);
            if matches!(cell, ">" | "^") {
                return None;
            }
            if index > 0 {
                output.push('\t');
            }
            output.push_str(&backlog_inline(
                cell.strip_prefix('~').unwrap_or(cell),
                origin,
            ));
        }
        if line.ends_with('\n') {
            output.push('\n');
        }
    }
    Some(output)
}

fn backlog_link_boundary(ch: char) -> bool {
    ch.is_whitespace() || matches!(ch, '(' | ')' | '[' | ']' | ',' | '「' | '」' | '、' | '。')
}

fn backlog_url_link(link: &str) -> Option<(&str, &str)> {
    let (label, url) = link
        .split_once('>')
        .or_else(|| {
            link.find(":https://")
                .or_else(|| link.find(":http://"))
                .map(|i| (&link[..i], &link[i + 1..]))
        })
        .unwrap_or((link, link));
    let parsed = reqwest::Url::parse(url).ok()?;
    (matches!(parsed.scheme(), "http" | "https") && parsed.host_str().is_some())
        .then_some((label, url))
}

fn backlog_unescaped_find(raw: &str, marker: &str) -> Option<usize> {
    raw.match_indices(marker).find_map(|(index, _)| {
        let before = &raw[..index];
        let slashes = before.len() - before.trim_end_matches('\\').len();
        slashes.is_multiple_of(2).then_some(index)
    })
}
