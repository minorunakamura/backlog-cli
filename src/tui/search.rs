use super::{SingleLineInput, browse::Project};
use crate::{
    Error,
    config::Settings,
    safety,
    service::{Candidate, IssueSearch, ResolvedIssueSearch, Selection, Service},
};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph},
};
use std::cell::Cell;

#[derive(Clone)]
pub(super) struct Choice {
    pub(super) id: Option<String>,
    pub(super) label: String,
}
impl Choice {
    fn display_label(&self, assignee: bool) -> &str {
        if assignee && self.id.is_none() && self.label == "self" {
            "自分"
        } else {
            &self.label
        }
    }
    fn selection(&self) -> Selection<'_> {
        self.id
            .as_deref()
            .map_or(Selection::Name(&self.label), Selection::Id)
    }
}
#[derive(Clone, Default)]
pub(super) struct Conditions {
    pub(super) keyword: String,
    pub(super) fields: [Vec<Choice>; 3],
}
impl Conditions {
    pub(super) fn defaults(settings: &Settings) -> Self {
        let mut value = Self::default();
        for name in &settings.issue.list.status {
            if !value.fields[0].iter().any(|choice| choice.label == *name) {
                value.fields[0].push(Choice {
                    id: None,
                    label: name.clone(),
                });
            }
        }
        value
    }
    pub(super) fn value(&self, field: usize) -> String {
        self.fields[field]
            .iter()
            .map(|choice| match &choice.id {
                Some(id) => format!("{} ({id})", choice.label),
                None => choice.display_label(field == 1).into(),
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
    pub(super) fn summary(&self, secrets: &[&str]) -> String {
        // Inspect the whole keyword before abbreviating, including any off-screen secret.
        let keyword = if self.keyword.chars().any(char::is_control)
            || secrets
                .iter()
                .any(|secret| !secret.is_empty() && self.keyword.contains(secret))
        {
            "[REDACTED]"
        } else if self.keyword.is_empty() {
            "Any"
        } else {
            &self.keyword
        };
        let line = Line::from(keyword);
        let graphemes: Vec<_> = line.styled_graphemes(Style::default()).collect();
        format!(
            "K:{}{} S:{} A:{} G:{}",
            graphemes
                .iter()
                .take(10)
                .map(|g| g.symbol)
                .collect::<String>(),
            if graphemes.len() > 10 { "…" } else { "" },
            self.fields[0].len(),
            self.fields[1].len(),
            self.fields[2].len()
        )
    }
    pub(super) async fn resolve(
        &mut self,
        service: &Service,
        project: &str,
    ) -> Result<ResolvedIssueSearch, Error> {
        if project != "all" {
            for (field, candidate) in self.fields.iter_mut().zip([
                Candidate::Status { project },
                Candidate::Assignee { project },
                Candidate::Milestone { project },
            ]) {
                for choice in field {
                    if choice.id.is_none() {
                        choice.id = Some(
                            service
                                .resolve(candidate, choice.selection())
                                .await?
                                .to_string(),
                        );
                    }
                }
            }
        }
        // Explicit editable values, including an empty status, must never reinsert local defaults.
        service
            .resolve_search(
                &Settings::default(),
                IssueSearch {
                    project: Some(project),
                    keyword: (!self.keyword.is_empty()).then_some(self.keyword.as_str()),
                    status: self.fields[0].iter().map(Choice::selection).collect(),
                    assignee: self.fields[1].iter().map(Choice::selection).collect(),
                    milestone: self.fields[2].iter().map(Choice::selection).collect(),
                    ..Default::default()
                },
            )
            .await
    }
}

pub(super) struct Dialog {
    pub(super) project: Option<Project>,
    pub(super) conditions: Conditions,
    keyword: SingleLineInput,
    cache: [Option<Vec<Choice>>; 3],
    field: usize,
    pub(super) applying: bool,
    pub(super) candidates: Option<Candidates>,
    value_offsets: [Cell<usize>; 3],
}
pub(super) struct Candidates {
    pub(super) items: Vec<Choice>,
    pub(super) loaded: bool,
    pub(super) failed: bool,
    selected: usize,
    top: Cell<usize>,
    horizontal: Cell<usize>,
}
pub(super) enum Action {
    None,
    Close,
    Apply,
    Candidates(usize),
    CancelCandidates,
}
impl Dialog {
    pub(super) fn new(project: Option<Project>, conditions: Conditions) -> Self {
        Self {
            project,
            keyword: SingleLineInput::new(conditions.keyword.clone()),
            conditions,
            cache: Default::default(),
            field: 0,
            applying: false,
            candidates: None,
            value_offsets: Default::default(),
        }
    }
    pub(super) fn event(&mut self, event: &Event, area: Rect) -> Action {
        let Event::Key(key) = event else {
            if !self.applying && self.field == 0 {
                self.keyword.event(event, area);
            }
            return Action::None;
        };
        if key.code == KeyCode::Esc && key.modifiers.is_empty() {
            return if self.candidates.take().is_some() {
                Action::CancelCandidates
            } else {
                Action::Close
            };
        }
        if self.applying {
            return Action::None;
        }
        if key.code == KeyCode::Enter {
            if !key.modifiers.is_empty() || key.kind != KeyEventKind::Press {
                return Action::None;
            }
            if self.candidates.as_ref().is_some_and(|c| c.failed) {
                return self.focus(self.field);
            }
            if self.candidates.take().is_some() {
                return Action::CancelCandidates;
            }
            self.conditions.keyword = self.keyword.value().into();
            return Action::Apply;
        }
        if self.field != 0 && key.modifiers == KeyModifiers::CONTROL {
            match key.code {
                KeyCode::Char('u') => {
                    self.conditions = Conditions::default();
                    self.keyword = SingleLineInput::new(String::new());
                    self.candidates = None;
                    for offset in &self.value_offsets {
                        offset.set(0);
                    }
                    return Action::CancelCandidates;
                }
                KeyCode::Char('k') => return self.focus(0),
                KeyCode::Char('s') => return self.focus(1),
                KeyCode::Char('a') => return self.focus(2),
                KeyCode::Char('g') => return self.focus(3),
                _ => {}
            }
        }
        if key.modifiers.is_empty() && key.code == KeyCode::Tab {
            return self.focus(if self.project.is_none() {
                if self.field == 0 { 2 } else { 0 }
            } else {
                (self.field + 1) % 4
            });
        }
        if key.code == KeyCode::BackTab && key.modifiers == KeyModifiers::SHIFT {
            return self.focus(if self.project.is_none() {
                if self.field == 0 { 2 } else { 0 }
            } else {
                (self.field + 3) % 4
            });
        }
        if self.field == 0 {
            self.keyword.event(event, area);
        } else if key.modifiers.is_empty() {
            if let Some(candidates) = &mut self.candidates {
                match key.code {
                    KeyCode::Down | KeyCode::Char('j') => {
                        candidates.selected =
                            (candidates.selected + 1).min(candidates.items.len().saturating_sub(1));
                        candidates.horizontal.set(0);
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        candidates.selected = candidates.selected.saturating_sub(1);
                        candidates.horizontal.set(0);
                    }
                    KeyCode::Left | KeyCode::Right => shift(&candidates.horizontal, key.code),
                    KeyCode::Char(' ') => {
                        if let Some(choice) = candidates.items.get(candidates.selected) {
                            let field = &mut self.conditions.fields[self.field - 1];
                            if let Some(index) = field
                                .iter()
                                .position(|c| c.selection() == choice.selection())
                            {
                                field.remove(index);
                            } else {
                                field.push(choice.clone());
                            }
                        }
                    }
                    _ => {}
                }
            } else if matches!(key.code, KeyCode::Left | KeyCode::Right) {
                shift(&self.value_offsets[self.field - 1], key.code);
            } else if matches!(
                key.code,
                KeyCode::Char(' ') | KeyCode::Down | KeyCode::Up | KeyCode::Char('j' | 'k')
            ) {
                return self.focus(self.field);
            }
        }
        Action::None
    }
    pub(super) fn load_candidates(&mut self, items: Vec<Choice>) {
        if let Some(candidates) = &mut self.candidates {
            for choice in &mut self.conditions.fields[self.field - 1] {
                if choice.id.is_none() {
                    let mut matches = items.iter().filter(|item| item.label == choice.label);
                    if let Some(item) = matches.next()
                        && matches.next().is_none()
                    {
                        choice.id = item.id.clone();
                    }
                }
            }
            self.cache[self.field - 1] = Some(items.clone());
            candidates.items = items;
            candidates.loaded = true;
        }
    }
    fn focus(&mut self, field: usize) -> Action {
        if self.project.is_none() && matches!(field, 1 | 3) {
            return Action::None;
        }
        if field == self.field
            && self
                .candidates
                .as_ref()
                .is_some_and(|c| !c.loaded && !c.failed)
        {
            return Action::None;
        }
        self.field = field;
        self.candidates = None;
        if field == 0 {
            return Action::CancelCandidates;
        }
        self.candidates = Some(Candidates {
            items: vec![],
            loaded: false,
            failed: false,
            selected: 0,
            top: Cell::new(0),
            horizontal: Cell::new(0),
        });
        if self.project.is_none() {
            let candidates = self.candidates.as_mut().unwrap();
            candidates.items = vec![Choice {
                id: None,
                label: "self".into(),
            }];
            candidates.loaded = true;
            Action::CancelCandidates
        } else if let Some(items) = &self.cache[field - 1] {
            let candidates = self.candidates.as_mut().unwrap();
            candidates.items = items.clone();
            candidates.loaded = true;
            Action::CancelCandidates
        } else {
            Action::Candidates(field - 1)
        }
    }

    pub(super) fn draw(&self, frame: &mut Frame<'_>, secrets: &[&str]) {
        let screen = frame.area();
        let width = screen.width.saturating_sub(2).min(72);
        let height = screen.height.saturating_sub(2).min(18);
        let area = Rect::new(
            screen.x + (screen.width - width) / 2,
            screen.y + (screen.height - height) / 2,
            width,
            height,
        );
        frame.render_widget(Clear, area);
        let block = Block::default()
            .borders(Borders::ALL)
            .title(if self.project.is_some() {
                "Search"
            } else {
                "Cross-project Search"
            });
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let wide = inner.width >= 60 && inner.height >= 9;
        let fields = if wide {
            [
                Rect::new(inner.x, inner.y, inner.width / 2, 2),
                Rect::new(
                    inner.x + inner.width / 2,
                    inner.y,
                    inner.width - inner.width / 2,
                    2,
                ),
                Rect::new(inner.x, inner.y + 2, inner.width, 2),
            ]
        } else {
            [
                Rect::new(inner.x, inner.y, inner.width, 1),
                Rect::new(inner.x, inner.y + 1, inner.width, 1),
                Rect::new(inner.x, inner.y + 2, inner.width, 1),
            ]
        };
        for (index, (label, field)) in ["状態", "担当者", "マイルストーン"]
            .iter()
            .zip(fields)
            .enumerate()
        {
            let label = format!(
                "{}{label}: ",
                if self.field == index + 1 { ">" } else { " " }
            );
            let label_width = Line::from(label.as_str()).width() as u16;
            frame.render_widget(Paragraph::new(label), field);
            let value_area = Rect::new(
                field.x + label_width,
                field.y,
                field.width.saturating_sub(label_width),
                1,
            );
            let value = if self.project.is_none() && index != 1 {
                "利用不可".into()
            } else {
                self.conditions.value(index)
            };
            draw_scrolled(
                frame,
                value_area,
                &value,
                &self.value_offsets[index],
                secrets,
            );
        }
        let keyword_area = Rect::new(inner.x, inner.y + if wide { 4 } else { 3 }, inner.width, 3);
        let keyword_block = Block::default()
            .borders(Borders::ALL)
            .title(if self.field == 0 {
                ">キーワード"
            } else {
                "キーワード"
            });
        let input_area = keyword_block.inner(keyword_area);
        frame.render_widget(keyword_block, keyword_area);
        self.keyword.draw(
            frame,
            input_area,
            self.field == 0 && !self.applying,
            secrets,
        );
        let help = if self.applying {
            "Applying... Esc: cancel"
        } else if self.field == 0 {
            "Tab/ShiftTab:移動\nEnter:検索 Esc:取消"
        } else {
            "Ctrl+K:入力 Ctrl+U:全クリア
Tab Enter:検索 Esc:取消 ←/→:全文"
        };
        frame.render_widget(
            Paragraph::new(safety::text(help, secrets)),
            Rect::new(inner.x, inner.bottom() - 2, inner.width, 2),
        );
        if let Some(candidates) = &self.candidates {
            let popup = Rect::new(
                inner.x,
                fields[self.field - 1]
                    .y
                    .min(inner.bottom() - inner.height.min(10)),
                inner.width.min(60),
                inner.height.min(10),
            );
            frame.render_widget(Clear, popup);
            let block = Block::default()
                .borders(Borders::ALL)
                .title(["状態", "担当者", "マイルストーン"][self.field - 1]);
            let content = block.inner(popup);
            frame.render_widget(block, popup);
            let rows = content.height.saturating_sub(3) as usize;
            let list = Rect::new(content.x, content.y, content.width, rows as u16);
            if candidates.failed {
                frame.render_widget(Paragraph::new("Candidates failed; Enter: retry"), list);
            } else if !candidates.loaded {
                frame.render_widget(Paragraph::new("Loading candidates..."), list);
            } else if candidates.items.is_empty() {
                frame.render_widget(Paragraph::new("No candidates"), list);
            } else {
                let top = candidates
                    .top
                    .get()
                    .min(candidates.selected)
                    .max(candidates.selected.saturating_sub(rows.saturating_sub(1)))
                    .min(candidates.items.len().saturating_sub(rows));
                candidates.top.set(top);
                for (row, (index, choice)) in candidates
                    .items
                    .iter()
                    .enumerate()
                    .skip(top)
                    .take(rows)
                    .enumerate()
                {
                    let checked = self.conditions.fields[self.field - 1]
                        .iter()
                        .any(|c| c.selection() == choice.selection());
                    let prefix = format!(
                        "{} [{}] ",
                        if index == candidates.selected {
                            ">"
                        } else {
                            " "
                        },
                        if checked { "x" } else { " " }
                    );
                    frame.render_widget(
                        Paragraph::new(prefix),
                        Rect::new(list.x, list.y + row as u16, 6, 1),
                    );
                    let value = format!(
                        "{}{}",
                        choice.display_label(self.field == 2),
                        choice
                            .id
                            .as_ref()
                            .map_or(String::new(), |id| format!(" ({id})"))
                    );
                    let unscrolled = Cell::new(0);
                    draw_scrolled(
                        frame,
                        Rect::new(
                            list.x + 6,
                            list.y + row as u16,
                            list.width.saturating_sub(6),
                            1,
                        ),
                        &value,
                        if index == candidates.selected {
                            &candidates.horizontal
                        } else {
                            &unscrolled
                        },
                        secrets,
                    );
                }
            }
            let help = if candidates.failed {
                "Enter:retry Esc:閉じる\nTab:移動 Ctrl+K:入力\nCtrl+U:全クリア"
            } else {
                "↑/↓ j/k Space:選択 ←/→:全文\nEnter/Esc:閉じる Tab:移動\nCtrl+K:入力 Ctrl+U:全クリア"
            };
            frame.render_widget(
                Paragraph::new(safety::text(help, secrets)),
                Rect::new(content.x, content.bottom() - 3, content.width, 3),
            );
        }
    }
}

fn shift(offset: &Cell<usize>, key: KeyCode) {
    offset.set(if key == KeyCode::Left {
        offset.get().saturating_sub(1)
    } else {
        offset.get().saturating_add(1)
    });
}

fn draw_scrolled(
    frame: &mut Frame<'_>,
    area: Rect,
    value: &str,
    offset: &Cell<usize>,
    secrets: &[&str],
) {
    let safe = safety::text(value, secrets)
        .replace('\n', "\\n")
        .replace('\t', "\\t");
    // Recheck display escapes too: a secret can match the generated text.
    let line = Line::from(safety::text(&safe, secrets));
    let graphemes: Vec<_> = line.styled_graphemes(Style::default()).collect();
    let start = offset.get().min(graphemes.len().saturating_sub(1));
    offset.set(start);
    let visible: String = graphemes.iter().skip(start).map(|g| g.symbol).collect();
    frame.render_widget(Paragraph::new(visible), area);
}
