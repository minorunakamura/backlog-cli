use super::{SingleLineInput, browse::Project};
use crate::{
    Error,
    config::{Config, Settings},
    safety,
    service::{Candidate, IssueCreate, Selection, Service},
};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph},
};

#[derive(Clone, PartialEq, Eq)]
struct Choice {
    id: String,
    label: String,
}
impl Choice {
    fn display(&self) -> String {
        format!("{} ({})", self.label, self.id)
    }
    fn selection(&self) -> Selection<'_> {
        Selection::Id(&self.id)
    }
}
#[derive(Clone, Default, PartialEq, Eq)]
struct Values {
    title: String,
    start: String,
    due: String,
    body: String,
    choices: [Vec<Choice>; 4],
}
pub(super) struct Loaded {
    options: [Vec<Choice>; 4],
    defaults: [Vec<Choice>; 4],
}
pub(super) async fn load(config: &Config, project: &str) -> Result<Loaded, Error> {
    let service = Service::new(config)?;
    let mut options: [Vec<Choice>; 4] = Default::default();
    for (slot, candidate) in options.iter_mut().zip([
        Candidate::IssueType { project },
        Candidate::Priority,
        Candidate::Assignee { project },
        Candidate::Milestone { project },
    ]) {
        *slot = service
            .candidates(candidate)
            .await?
            .as_array()
            .unwrap()
            .iter()
            .map(|item| Choice {
                id: item["id"].to_string(),
                label: item["name"].as_str().unwrap().into(),
            })
            .collect();
    }
    let mut defaults: [Vec<Choice>; 4] = Default::default();
    let configured = &config.settings.issue.create;
    for (index, name) in [
        &configured.issue_type,
        &configured.priority,
        &configured.assignee,
    ]
    .into_iter()
    .enumerate()
    {
        if let Some(name) = name {
            let choice = if index == 2 && name == "self" {
                let id = service.myself().await?["id"].to_string();
                options[index]
                    .iter()
                    .find(|c| c.id == id)
                    .cloned()
                    .unwrap_or(Choice {
                        id,
                        label: "自分".into(),
                    })
            } else {
                let matches: Vec<_> = options[index].iter().filter(|c| c.label == *name).collect();
                if matches.len() != 1 {
                    return Err(Error::new(
                        "input",
                        "Create default has no unique exact candidate",
                    ));
                }
                matches[0].clone()
            };
            defaults[index].push(choice);
        }
    }
    Ok(Loaded { options, defaults })
}

#[derive(PartialEq)]
enum Phase {
    Loading,
    LoadFailed,
    Form,
    Confirm,
    Discard,
    Sending,
    Result,
}
struct Popup {
    index: usize,
    horizontal: usize,
}
pub(super) enum Action {
    None,
    Close,
    Editor,
}
pub(super) struct Create {
    pub(super) project: Project,
    identity: String,
    title: SingleLineInput,
    start: SingleLineInput,
    due: SingleLineInput,
    pub(super) body: String,
    choices: [Vec<Choice>; 4],
    options: [Vec<Choice>; 4],
    initial: Values,
    field: usize,
    phase: Phase,
    popup: Option<Popup>,
    scroll: std::cell::Cell<usize>,
    pub(super) notice: String,
    result: String,
    pub(super) refreshing: bool,
}
impl Create {
    pub(super) fn new(project: Project, profile: &str, origin: &str, space: &str) -> Self {
        Self {
            identity: format!(
                "Profile: {profile}\nOrigin: {origin}\nSpace: {space}\nProject: {} [{}] (ID {})",
                project.key, project.label, project.id
            ),
            project,
            title: SingleLineInput::new(String::new()),
            start: SingleLineInput::new(String::new()),
            due: SingleLineInput::new(String::new()),
            body: String::new(),
            choices: Default::default(),
            options: Default::default(),
            initial: Values::default(),
            field: 0,
            phase: Phase::Loading,
            popup: None,
            scroll: std::cell::Cell::new(0),
            notice: String::new(),
            result: String::new(),
            refreshing: false,
        }
    }
    fn values(&self) -> Values {
        Values {
            title: self.title.value().into(),
            start: self.start.value().into(),
            due: self.due.value().into(),
            body: self.body.clone(),
            choices: self.choices.clone(),
        }
    }
    pub(super) fn loading(&self) -> bool {
        self.phase == Phase::Loading
    }
    pub(super) fn sending(&self) -> bool {
        self.phase == Phase::Sending
    }
    pub(super) fn loaded(&mut self, loaded: Result<Loaded, Error>) {
        match loaded {
            Ok(loaded) => {
                self.options = loaded.options;
                self.choices = loaded.defaults;
                self.initial = self.values();
                self.phase = Phase::Form;
                self.notice.clear();
            }
            Err(error) => {
                self.notice = error.message;
                self.phase = Phase::LoadFailed;
            }
        }
    }
    pub(super) fn submission(&self) -> Submission {
        Submission {
            values: self.values(),
            project: self.project.id.clone(),
        }
    }
    pub(super) fn finished(&mut self, result: Result<serde_json::Value, Error>) -> bool {
        self.phase = Phase::Result;
        self.scroll.set(0);
        match result {
            Ok(value) => {
                self.result = format!(
                    "Created: {}\n{}\nRefreshing current search...",
                    value["issueKey"].as_str().unwrap(),
                    value["url"].as_str().unwrap()
                );
                self.refreshing = true;
                true
            }
            Err(error) => {
                let outcome = match error.write.map(|w| w.outcome) {
                    Some(crate::WriteOutcome::Created) => {
                        "Created; response processing failed. Do not resend."
                    }
                    Some(crate::WriteOutcome::Unknown) => {
                        "Unknown: creation result unknown. Do not resend."
                    }
                    _ => "NotCreated: issue not created.",
                };
                self.result = format!("{outcome}\n{}", error.message);
                false
            }
        }
    }
    pub(super) fn refreshed(&mut self, result: Result<(), String>) {
        self.refreshing = false;
        if let Some(prefix) = self.result.strip_suffix("\nRefreshing current search...") {
            self.result = prefix.into();
        }
        self.result.push_str(&match result {
            Ok(()) => "\nCreated; list refreshed".into(),
            Err(error) => {
                format!("\nCreated; list refresh failed / 作成済み・一覧更新失敗\n{error}")
            }
        });
    }
    fn choice_field(&self) -> Option<usize> {
        match self.field {
            1..=3 => Some(self.field - 1),
            6 => Some(3),
            _ => None,
        }
    }
    fn changed(&self) -> bool {
        self.values() != self.initial
    }
    fn cancel(&mut self) -> Action {
        if self.changed() {
            self.phase = Phase::Discard;
            Action::None
        } else {
            Action::Close
        }
    }
    pub(super) fn event(&mut self, event: &Event, area: Rect) -> Action {
        let Event::Key(key) = event else {
            if self.phase == Phase::Form && self.popup.is_none() {
                self.input_event(event, area);
            }
            return Action::None;
        };
        if key.kind == KeyEventKind::Release {
            return Action::None;
        }
        let plain = key.modifiers.is_empty();
        let activate = plain
            && key.kind == KeyEventKind::Press
            && matches!(key.code, KeyCode::Enter | KeyCode::Char(' '));
        match self.phase {
            Phase::Sending => return Action::None,
            Phase::Loading | Phase::LoadFailed => {
                if plain && key.code == KeyCode::Esc {
                    return Action::Close;
                }
                if activate && self.phase == Phase::LoadFailed {
                    self.phase = Phase::Loading;
                }
                return Action::None;
            }
            Phase::Discard => {
                if plain && key.code == KeyCode::Esc {
                    self.phase = Phase::Form;
                }
                return if activate {
                    Action::Close
                } else {
                    Action::None
                };
            }
            Phase::Confirm | Phase::Result => {
                if plain && key.code == KeyCode::Esc && self.phase == Phase::Confirm {
                    self.phase = Phase::Form;
                    self.scroll.set(0);
                    return Action::None;
                }
                if self.phase == Phase::Result
                    && !self.refreshing
                    && plain
                    && matches!(key.code, KeyCode::Enter | KeyCode::Esc)
                {
                    return Action::Close;
                }
                if self.phase == Phase::Confirm
                    && plain
                    && key.kind == KeyEventKind::Press
                    && key.code == KeyCode::Char('s')
                {
                    self.phase = Phase::Sending;
                    return Action::None;
                }
                if plain || key.modifiers == KeyModifiers::SHIFT {
                    self.scroll.set(match key.code {
                        KeyCode::Down | KeyCode::Char('j') => self.scroll.get().saturating_add(1),
                        KeyCode::Up | KeyCode::Char('k') => self.scroll.get().saturating_sub(1),
                        KeyCode::PageDown | KeyCode::Char(' ') => self
                            .scroll
                            .get()
                            .saturating_add(area.height.saturating_sub(6) as usize),
                        KeyCode::PageUp => self
                            .scroll
                            .get()
                            .saturating_sub(area.height.saturating_sub(6) as usize),
                        KeyCode::Home | KeyCode::Char('g') => 0,
                        KeyCode::End | KeyCode::Char('G') => usize::MAX,
                        _ => self.scroll.get(),
                    });
                }
                return Action::None;
            }
            Phase::Form => {}
        }
        if let Some(popup) = &mut self.popup {
            let field = match self.field {
                6 => 3,
                _ => self.field - 1,
            };
            let length = self.options[field].len() + usize::from(field == 2);
            if plain {
                match key.code {
                    KeyCode::Esc => self.popup = None,
                    KeyCode::Down | KeyCode::Char('j') => {
                        popup.index = (popup.index + 1).min(length.saturating_sub(1));
                        popup.horizontal = 0;
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        popup.index = popup.index.saturating_sub(1);
                        popup.horizontal = 0;
                    }
                    KeyCode::Left => popup.horizontal = popup.horizontal.saturating_sub(1),
                    KeyCode::Right => popup.horizontal = popup.horizontal.saturating_add(1),
                    _ if activate => {
                        if field == 3 && key.code == KeyCode::Enter {
                            self.popup = None;
                        } else if field == 2 && popup.index == 0 {
                            self.choices[field].clear();
                            self.popup = None;
                        } else if let Some(choice) = self.options[field]
                            .get(popup.index - usize::from(field == 2))
                            .cloned()
                        {
                            if field == 3 {
                                if let Some(index) =
                                    self.choices[field].iter().position(|c| c.id == choice.id)
                                {
                                    self.choices[field].remove(index);
                                } else {
                                    self.choices[field].push(choice);
                                }
                            } else {
                                self.choices[field] = vec![choice];
                                self.popup = None;
                            }
                        }
                    }
                    _ => {}
                }
            }
            return Action::None;
        }
        if key.modifiers == KeyModifiers::CONTROL {
            let target = match key.code {
                KeyCode::Char('t') => Some(0),
                KeyCode::Char('y') => Some(1),
                KeyCode::Char('p') => Some(2),
                KeyCode::Char('a') => Some(3),
                KeyCode::Char('b') => Some(7),
                _ => None,
            };
            if let Some(field) = target {
                self.field = field;
                self.notice.clear();
                return Action::None;
            }
        }
        match key.code {
            KeyCode::Esc if plain => return self.cancel(),
            KeyCode::Tab if plain => self.field = (self.field + 1) % 10,
            KeyCode::BackTab if key.modifiers == KeyModifiers::SHIFT || plain => {
                self.field = (self.field + 9) % 10
            }
            KeyCode::Delete | KeyCode::Backspace if plain && self.choice_field().is_some() => {
                self.choices[self.choice_field().unwrap()].clear();
            }
            KeyCode::Delete | KeyCode::Backspace if plain && self.field == 7 => self.body.clear(),
            _ if activate && self.choice_field().is_some() => {
                self.popup = Some(Popup {
                    index: 0,
                    horizontal: 0,
                })
            }
            _ if activate && self.field == 7 => return Action::Editor,
            _ if activate && self.field == 8 => {
                if self.title.value().trim().is_empty() {
                    self.notice = "Title must not be blank".into();
                } else if self.choices[0].is_empty() || self.choices[1].is_empty() {
                    self.notice = "Issue type and priority are required".into();
                } else if [self.start.value(), self.due.value()]
                    .iter()
                    .any(|d| !d.is_empty() && !crate::service::valid_date(d))
                {
                    self.notice = "Date must be a real YYYY-MM-DD date".into();
                } else {
                    self.phase = Phase::Confirm;
                    self.scroll.set(0);
                    self.notice.clear();
                }
            }
            _ if activate && self.field == 9 => return self.cancel(),
            _ => self.input_event(event, area),
        }
        Action::None
    }
    fn input_event(&mut self, event: &Event, area: Rect) {
        match self.field {
            0 => self.title.event(event, area),
            4 => self.start.event(event, area),
            5 => self.due.event(event, area),
            _ => {}
        }
    }
    fn selection_text(&self, index: usize) -> String {
        if self.choices[index].is_empty() {
            "(cleared)".into()
        } else {
            self.choices[index]
                .iter()
                .map(Choice::display)
                .collect::<Vec<_>>()
                .join(", ")
        }
    }
    fn confirmation(&self) -> String {
        format!(
            "{}\nTitle: {}\nIssue type: {}\nPriority: {}\nAssignee: {}\nStart date: {}\nDue date: {}\nMilestones: {}\nBody:\n{}\n[End of all input]",
            self.identity,
            self.title.value(),
            self.selection_text(0),
            self.selection_text(1),
            self.selection_text(2),
            if self.start.value().is_empty() {
                "(cleared)"
            } else {
                self.start.value()
            },
            if self.due.value().is_empty() {
                "(cleared)"
            } else {
                self.due.value()
            },
            self.selection_text(3),
            if self.body.is_empty() {
                "(empty / cleared)"
            } else {
                &self.body
            }
        )
    }
    pub(super) fn draw(&self, frame: &mut Frame<'_>, secrets: &[&str]) {
        let screen = frame.area();
        let area = Rect::new(1, 1, screen.width - 2, screen.height - 2);
        let title = match self.phase {
            Phase::Confirm => "Confirm create (no POST yet)",
            Phase::Sending => "Sending - Ctrl+C: interrupt",
            Phase::Result => "Create result",
            Phase::Discard => "Discard changes?",
            _ => "Create issue",
        };
        frame.render_widget(Clear, area);
        frame.render_widget(Block::default().borders(Borders::ALL).title(title), area);
        let inner = Rect::new(area.x + 1, area.y + 1, area.width - 2, area.height - 2);
        let safe = |text: &str| {
            crate::tui_text::display(Text::from(safety::text(text, secrets)), secrets).to_string()
        };
        if self.phase == Phase::Discard {
            frame.render_widget(
                Paragraph::new("Discard changes?\nEnter/Space: discard  Esc: keep editing"),
                inner,
            );
            return;
        }
        if self.phase == Phase::Loading || self.phase == Phase::LoadFailed {
            frame.render_widget(
                Paragraph::new(safe(&format!(
                    "Project: {}\n{}\n{}",
                    self.project.label,
                    if self.loading() {
                        "Loading create candidates..."
                    } else {
                        &self.notice
                    },
                    "Enter: retry  Esc: cancel"
                ))),
                inner,
            );
            return;
        }
        if matches!(self.phase, Phase::Confirm | Phase::Sending | Phase::Result) {
            let content = if self.phase == Phase::Result {
                self.result.clone()
            } else {
                self.confirmation()
            };
            let body = Rect::new(inner.x, inner.y, inner.width, inner.height - 2);
            let lines = wrapped_lines(&content, body.width, secrets);
            let offset = self
                .scroll
                .get()
                .min(lines.len().saturating_sub(body.height as usize));
            self.scroll.set(offset);
            frame.render_widget(
                Paragraph::new(
                    lines
                        .into_iter()
                        .skip(offset)
                        .take(body.height as usize)
                        .collect::<Vec<_>>()
                        .join("\n"),
                ),
                body,
            );
            let help = if self.phase == Phase::Confirm {
                "j/k PgUp/Dn Home/End: read all\ns SEND ONCE Esc edit (Enter: no send)"
            } else if self.sending() {
                "Sending: edit/send/discard disabled\nCtrl+C: interrupt"
            } else if self.refreshing {
                "Refreshing: please wait / Ctrl+C: interrupt"
            } else {
                "j/k PgUp/Dn Home/End: read all\nEnter/Esc: Issue List"
            };
            frame.render_widget(
                Paragraph::new(help),
                Rect::new(inner.x, inner.y + inner.height - 2, inner.width, 2),
            );
            return;
        }
        frame.render_widget(
            Paragraph::new(safe(&format!(
                "Project: {} (ID {})",
                self.project.label, self.project.id
            ))),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );
        let labels = [
            "Title",
            "Issue type",
            "Priority",
            "Assignee",
            "Start date",
            "Due date",
            "Milestones",
            "Body edit",
            "Review / 確認へ",
            "Cancel",
        ];
        let visible = inner.height.saturating_sub(4) as usize;
        let top = self.field.saturating_sub(visible.saturating_sub(1));
        for (row, index) in (top..10).take(visible).enumerate() {
            let value = match index {
                0 => String::new(),
                1..=3 => self.selection_text(index - 1),
                4 | 5 => String::new(),
                6 => self.selection_text(3),
                7 => format!(
                    "{} chars (Enter: editor / Delete: clear)",
                    self.body.chars().count()
                ),
                _ => String::new(),
            };
            let prefix = format!(
                "{} {}: ",
                if index == self.field { ">" } else { " " },
                labels[index]
            );
            let line_area = Rect::new(inner.x, inner.y + 1 + row as u16, inner.width, 1);
            frame.render_widget(Paragraph::new(safe(&format!("{prefix}{value}"))), line_area);
            if matches!(index, 0 | 4 | 5) {
                let width = Line::from(prefix.clone()).width().min(inner.width as usize) as u16;
                let input_area =
                    Rect::new(line_area.x + width, line_area.y, line_area.width - width, 1);
                let input = match index {
                    0 => &self.title,
                    4 => &self.start,
                    _ => &self.due,
                };
                input.draw(frame, input_area, self.field == index, secrets);
            }
        }
        frame.render_widget(
            Paragraph::new(safe(&self.notice)),
            Rect::new(inner.x, inner.y + inner.height - 3, inner.width, 1),
        );
        frame.render_widget(
            Paragraph::new("Tab/S-Tab focus Esc cancel\nEnter/Space action Del clear"),
            Rect::new(inner.x, inner.y + inner.height - 2, inner.width, 2),
        );
        if let Some(popup) = &self.popup {
            let field = self.choice_field().unwrap();
            frame.render_widget(Clear, inner);
            frame.render_widget(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!("{} candidates", labels[self.field])),
                inner,
            );
            let rows = inner.height.saturating_sub(4) as usize;
            let top = popup.index.saturating_sub(rows.saturating_sub(1));
            let mut items = self.options[field]
                .iter()
                .map(|c| {
                    format!(
                        "{} {}",
                        if self.choices[field].contains(c) {
                            "[x]"
                        } else {
                            "[ ]"
                        },
                        c.display()
                    )
                })
                .collect::<Vec<_>>();
            if field == 2 {
                items.insert(0, "(cleared)".into());
            }
            if items.is_empty() {
                items.push("No candidates".into());
            }
            for (row, (index, text)) in items.iter().enumerate().skip(top).take(rows).enumerate() {
                let safe_text = safe(text);
                let line = Line::from(safe_text);
                let visible: String = line
                    .styled_graphemes(Style::default())
                    .skip(popup.horizontal)
                    .map(|g| g.symbol)
                    .collect();
                frame.render_widget(
                    Paragraph::new(format!(
                        "{} {visible}",
                        if index == popup.index { ">" } else { " " }
                    )),
                    Rect::new(inner.x + 1, inner.y + 1 + row as u16, inner.width - 2, 1),
                );
            }
            let help = if field == 3 {
                "j/k move Space toggle\nEnter/Esc close Left/Right read"
            } else {
                "j/k move Enter/Space choose\nEsc close Left/Right read"
            };
            frame.render_widget(
                Paragraph::new(help),
                Rect::new(inner.x + 1, inner.y + inner.height - 3, inner.width - 2, 2),
            );
        }
    }
}
pub(super) struct Submission {
    values: Values,
    project: String,
}
impl Submission {
    pub(super) async fn run(self, service: Service) -> Result<serde_json::Value, Error> {
        self.values.send(&self.project, &service).await
    }
}
impl Values {
    async fn send(&self, project: &str, service: &Service) -> Result<serde_json::Value, Error> {
        service
            .issue_create(
                &Settings::default(),
                IssueCreate {
                    project: Some(project),
                    title: &self.title,
                    body: Some(&self.body),
                    issue_type: self.choices[0].first().map(Choice::selection),
                    priority: self.choices[1].first().map(Choice::selection),
                    assignee: self.choices[2].first().map(Choice::selection),
                    clear_assignee: self.choices[2].is_empty(),
                    milestone: self.choices[3].iter().map(Choice::selection).collect(),
                    start_date: (!self.start.is_empty()).then_some(self.start.as_str()),
                    due_date: (!self.due.is_empty()).then_some(self.due.as_str()),
                },
            )
            .await
    }
}
// Wrap before slicing so the complete confirmation is accessible beyond Paragraph's u16 scroll limit.
fn wrapped_lines(content: &str, width: u16, secrets: &[&str]) -> Vec<String> {
    let text = crate::tui_text::display(Text::from(safety::text(content, secrets)), secrets);
    let mut result = Vec::new();
    for line in text.lines {
        let mut current = String::new();
        let mut columns = 0;
        for grapheme in line.styled_graphemes(Style::default()) {
            let size = Span::raw(grapheme.symbol).width();
            if columns + size > width as usize && !current.is_empty() {
                result.push(std::mem::take(&mut current));
                columns = 0;
            }
            current.push_str(grapheme.symbol);
            columns += size;
        }
        result.push(current);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::Shell;
    use crossterm::event::KeyEvent;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn create_candidate_popup_and_selected_form_do_not_reconstruct_secrets() {
        for (width, height) in [(40, 12), (100, 24)] {
            let area = Rect::new(0, 0, width, height);
            let mut shell = Shell::new(true);
            shell.browse.secret = "test-secret".into();
            let mut create = Create::new(
                Project {
                    id: "1".into(),
                    key: "LOCAL".into(),
                    label: "Local project".into(),
                },
                "local-check",
                "https://localhost:9",
                "Local space",
            );
            create.phase = Phase::Form;
            create.options[0].push(Choice {
                id: "2".into(),
                label: "test-\u{200b}secret".into(),
            });
            shell.create = Some(create);
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            let screen = |shell: &Shell, terminal: &mut Terminal<TestBackend>| {
                terminal.draw(|frame| shell.draw(frame)).unwrap();
                terminal
                    .backend()
                    .buffer()
                    .content
                    .iter()
                    .map(|cell| cell.symbol())
                    .collect::<String>()
            };
            for code in [KeyCode::Tab, KeyCode::Enter] {
                shell.key(KeyEvent::new(code, KeyModifiers::NONE), area);
            }
            let popup = screen(&shell, &mut terminal);
            assert!(popup.contains("Issue type candidates"), "{popup}");
            assert!(!popup.contains("test-secret"), "{popup}");
            assert!(popup.contains("test-\\u{200b}secret"), "{popup}");
            for _ in 0..30 {
                shell.key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE), area);
                let popup = screen(&shell, &mut terminal);
                assert!(!popup.contains("test-secret"), "{popup}");
            }
            for _ in 0..30 {
                shell.key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE), area);
            }
            assert!(screen(&shell, &mut terminal).contains("test-\\u{200b}secret"));
            shell.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), area);
            let selected = screen(&shell, &mut terminal);
            assert!(
                selected.contains("> Issue type: test-\\u{200b}"),
                "{selected}"
            );
            assert!(!selected.contains("test-secret"), "{selected}");
        }
    }
}
