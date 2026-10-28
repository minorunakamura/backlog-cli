use crate::{body, config::Config, safety};
use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    layout::Rect,
    widgets::{Paragraph, Wrap},
};
use serde_json::Value;
use std::{borrow::Cow, cell::Cell, time::Instant};

#[derive(Default)]
pub(super) struct Detail {
    pub(super) key: Option<String>,
    text: Option<String>,
    pub(super) due: Option<Instant>,
    pub(super) status: &'static str,
    pub(super) notice: String,
    scroll: Cell<u16>,
}
impl Detail {
    pub(super) fn select(&mut self, key: Option<String>) {
        *self = Self {
            key,
            status: "Loading preview...",
            due: Some(Instant::now() + std::time::Duration::from_millis(150)),
            ..Self::default()
        };
    }
    /// Accept the safe display copy produced by `display`, never raw API text.
    pub(super) fn replace(&mut self, text: String) {
        self.text = Some(text);
        self.status = "";
        self.scroll.set(0);
    }
    fn paragraph(&self, secret: &str) -> Paragraph<'_> {
        let content = match (&self.key, &self.text) {
            (_, Some(text)) => Cow::Borrowed(text.as_str()),
            (Some(key), None) => {
                Cow::Owned(safety::text(&format!("{key}\n{}", self.status), &[secret]))
            }
            _ => Cow::Owned(safety::text("Select an issue", &[secret])),
        };
        Paragraph::new(content).wrap(Wrap { trim: false })
    }
    fn maximum(paragraph: &Paragraph<'_>, area: Rect) -> u16 {
        // shortcut: Paragraph's scroll is u16; revisit for documents over 65,535 wrapped rows.
        paragraph
            .line_count(area.width)
            .saturating_sub(area.height as usize)
            .min(u16::MAX as usize) as u16
    }
    pub(super) fn key(&mut self, code: KeyCode, area: Rect, secret: &str) {
        let maximum = Self::maximum(&self.paragraph(secret), area);
        let scroll = self.scroll.get().min(maximum);
        let page = area.height.max(1);
        self.scroll.set(match code {
            KeyCode::Down | KeyCode::Char('j') => scroll.saturating_add(1).min(maximum),
            KeyCode::Up | KeyCode::Char('k') => scroll.saturating_sub(1),
            KeyCode::PageDown | KeyCode::Char(' ') => scroll.saturating_add(page).min(maximum),
            KeyCode::PageUp | KeyCode::Char('b') => scroll.saturating_sub(page),
            KeyCode::Home | KeyCode::Char('g') => 0,
            KeyCode::End | KeyCode::Char('G') => maximum,
            _ => scroll,
        });
    }
    pub(super) fn draw(&self, frame: &mut Frame<'_>, area: Rect, secret: &str) {
        let paragraph = self.paragraph(secret);
        self.scroll
            .set(self.scroll.get().min(Self::maximum(&paragraph, area)));
        frame.render_widget(paragraph.scroll((self.scroll.get(), 0)), area);
    }
}

pub(super) fn display(value: &Value, rule: &str, config: &Config) -> String {
    let mut text = String::new();
    for (label, field) in [
        ("KEY", "issueKey"),
        ("TITLE", "summary"),
        ("TYPE", "issueType"),
        ("PRIORITY", "priority"),
        ("ASSIGNEE", "assignee"),
        ("STATUS", "status"),
        ("RESOLUTION", "resolution"),
        ("START", "startDate"),
        ("DUE", "dueDate"),
    ] {
        let value = value[field]
            .as_str()
            .or_else(|| value[field]["name"].as_str())
            .unwrap_or("-");
        text.push_str(&format!("{label}: {value}\n"));
    }
    let milestones = value["milestone"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["name"].as_str().unwrap())
        .collect::<Vec<_>>()
        .join(", ");
    text.push_str(&format!(
        "MILESTONES: {milestones}\nCREATED: {}\nUPDATED: {}\nBODY\n",
        value["created"].as_str().unwrap(),
        value["updated"].as_str().unwrap()
    ));
    text.push_str(&body::display_with_origin(
        value["description"].as_str().unwrap_or(""),
        rule,
        &config.origin,
        &[&config.api_key],
    ));
    safety::text(&text, &[&config.api_key])
}
