use super::{SingleLineInput, browse::Project};
use crate::{
    Error,
    config::{Config, Settings},
    safety,
    service::{
        Candidate, CommentAdd, IssueCreate, IssueUpdate, PreparedCommentAdd, PreparedIssueUpdate,
        Selection, Service,
    },
};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph},
};

#[derive(Clone, Eq)]
struct Choice {
    id: String,
    label: String,
}
impl PartialEq for Choice {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
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
    choices: [Vec<Choice>; 6],
}
pub(super) struct Loaded {
    options: [Vec<Choice>; 6],
    defaults: [Vec<Choice>; 6],
    edit: Option<EditLoaded>,
}
struct EditLoaded {
    issue: serde_json::Value,
    rule: String,
}
pub(super) async fn load_update(
    config: &Config,
    key: &str,
    comment: bool,
) -> Result<Loaded, Error> {
    let service = Service::new(config)?;
    let issue = service.issue_view(key).await?;
    if issue["issueKey"].as_str() != Some(key) {
        return Err(Error::new(
            "invalid_response",
            "Issue response does not match selected key",
        ));
    }
    let project = issue["projectId"].to_string();
    let rule = service
        .project_text_formatting_rule(issue["projectId"].as_number().unwrap())
        .await?;
    let mut options: [Vec<Choice>; 6] = Default::default();
    for (index, (slot, candidate)) in options
        .iter_mut()
        .zip([
            Candidate::IssueType { project: &project },
            Candidate::Priority,
            Candidate::Assignee { project: &project },
            Candidate::Milestone { project: &project },
            Candidate::Status { project: &project },
            Candidate::Resolution,
        ])
        .enumerate()
    {
        if comment && !matches!(index, 2 | 4 | 5) {
            continue;
        }
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
    let mut defaults: [Vec<Choice>; 6] = Default::default();
    for (index, field) in [
        "issueType",
        "priority",
        "assignee",
        "milestone",
        "status",
        "resolution",
    ]
    .iter()
    .enumerate()
    {
        if comment && !matches!(index, 2 | 4 | 5) {
            continue;
        }
        let values: Vec<_> = if index == 3 {
            issue[*field].as_array().unwrap().iter().collect()
        } else if issue[*field].is_null() {
            vec![]
        } else {
            vec![&issue[*field]]
        };
        for value in values {
            let id = value["id"]
                .as_number()
                .map(ToString::to_string)
                .filter(|id| {
                    if index == 5 {
                        id == "0" || crate::service::positive_integer(id)
                    } else {
                        crate::service::positive_integer(id)
                    }
                })
                .ok_or_else(|| {
                    Error::new("invalid_response", "Issue response has invalid editable ID")
                })?;
            defaults[index].push(Choice {
                id,
                label: value["name"].as_str().unwrap().into(),
            });
        }
    }
    // Current values can be absent from candidates (for example, an archived milestone).
    for (options, current) in options.iter_mut().zip(&defaults) {
        for choice in current {
            if !options.iter().any(|c| c.id == choice.id) {
                options.push(choice.clone());
            }
        }
    }
    Ok(Loaded {
        options,
        defaults,
        edit: Some(EditLoaded { issue, rule }),
    })
}
pub(super) async fn load(config: &Config, project: &str) -> Result<Loaded, Error> {
    let service = Service::new(config)?;
    let mut options: [Vec<Choice>; 6] = Default::default();
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
    let mut defaults: [Vec<Choice>; 6] = Default::default();
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
    Ok(Loaded {
        options,
        defaults,
        edit: None,
    })
}

#[derive(PartialEq)]
enum Phase {
    Loading,
    LoadFailed,
    Form,
    Preparing,
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
    choices: [Vec<Choice>; 6],
    options: [Vec<Choice>; 6],
    initial: Values,
    field: usize,
    phase: Phase,
    popup: Option<Popup>,
    scroll: std::cell::Cell<usize>,
    pub(super) notice: String,
    result: String,
    pub(super) refreshing: bool,
    pub(super) update_key: Option<String>,
    pub(super) comment: bool,
    edit: Option<EditLoaded>,
    prepared: Option<Prepared>,
}
#[derive(Clone)]
pub(super) enum Prepared {
    Issue(PreparedIssueUpdate),
    Comment(PreparedCommentAdd),
}
impl Prepared {
    fn changes(&self) -> &[(&'static str, String)] {
        match self {
            Self::Issue(p) => p.changes(),
            Self::Comment(p) => p.changes(),
        }
    }
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
            update_key: None,
            edit: None,
            prepared: None,
            comment: false,
        }
    }
    pub(super) fn new_update(key: String, profile: &str, origin: &str, space: &str) -> Self {
        let mut form = Self::new(
            Project {
                id: String::new(),
                key: key.clone(),
                label: key.clone(),
            },
            profile,
            origin,
            space,
        );
        form.identity =
            format!("Profile: {profile}\nOrigin: {origin}\nSpace: {space}\nIssue: {key}");
        form.update_key = Some(key);
        form
    }
    pub(super) fn new_comment(key: String, profile: &str, origin: &str, space: &str) -> Self {
        let mut form = Self::new_update(key, profile, origin, space);
        form.comment = true;
        form.field = 7;
        form
    }
    fn fields(&self) -> Vec<usize> {
        if self.comment {
            vec![7, 8, 3, 9, 10, 11]
        } else {
            (0..self.field_count()).collect()
        }
    }
    pub(super) fn send_outcome(&self) -> crate::WriteOutcome {
        match &self.prepared {
            Some(Prepared::Comment(p)) => p.outcome(),
            _ if self.update_key.is_some() => crate::WriteOutcome::NotUpdated,
            _ => crate::WriteOutcome::NotCreated,
        }
    }
    fn field_count(&self) -> usize {
        if self.update_key.is_some() { 12 } else { 10 }
    }
    pub(super) fn preparing(&self) -> bool {
        self.phase == Phase::Preparing
    }
    pub(super) fn preparation(&self) -> Preparation {
        Preparation {
            values: self.values(),
            initial: self.initial.clone(),
            issue: self.edit.as_ref().unwrap().issue.clone(),
            key: self.update_key.clone().unwrap(),
            comment: self.comment,
        }
    }
    pub(super) fn prepared(&mut self, result: Result<Prepared, Error>) {
        match result {
            Ok(prepared) => {
                self.prepared = Some(prepared);
                self.phase = Phase::Confirm;
                self.scroll.set(0);
                self.notice.clear();
            }
            Err(error) => {
                self.phase = Phase::Form;
                self.notice = error.message;
            }
        }
    }
    pub(super) fn update_display(
        &self,
        value: &serde_json::Value,
        config: &Config,
    ) -> Result<String, Error> {
        crate::service::validate_issue_view(value)
            .map_err(|error| error.with_update_outcome(crate::WriteOutcome::Updated))?;
        if value["issueKey"].as_str() != self.update_key.as_deref() {
            return Err(Error::new(
                "invalid_response",
                "Issue response does not match selected key",
            )
            .with_update_outcome(crate::WriteOutcome::Updated));
        }
        Ok(super::detail::display(
            value,
            &self.edit.as_ref().unwrap().rule,
            config,
        ))
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
                if let Some(edit) = loaded.edit {
                    if !self.comment {
                        self.title =
                            SingleLineInput::new(edit.issue["summary"].as_str().unwrap().into());
                        self.body = edit.issue["description"]
                            .as_str()
                            .unwrap_or_default()
                            .into();
                        let date = |field: &str| -> Result<String, Error> {
                            if edit.issue[field].is_null() {
                                return Ok(String::new());
                            }
                            edit.issue[field]
                                .as_str()
                                .and_then(|date| date.get(..10))
                                .filter(|date| crate::service::valid_date(date))
                                .map(str::to_owned)
                                .ok_or_else(|| {
                                    Error::new(
                                        "invalid_response",
                                        "Issue response has invalid editable date",
                                    )
                                })
                        };
                        let dates = date("startDate")
                            .and_then(|start| date("dueDate").map(|due| (start, due)));
                        match dates {
                            Ok((start, due)) => {
                                self.start = SingleLineInput::new(start);
                                self.due = SingleLineInput::new(due);
                            }
                            Err(error) => {
                                self.notice = error.message;
                                self.phase = Phase::LoadFailed;
                                return;
                            }
                        }
                    }
                    self.project.id = edit.issue["projectId"].to_string();
                    self.project.label = self.project.id.clone();
                    self.edit = Some(edit);
                }
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
    pub(super) fn submission(&mut self) -> Submission {
        Submission {
            values: self.values(),
            project: self.project.id.clone(),
            prepared: self.prepared.clone(),
        }
    }
    pub(super) fn finished(&mut self, result: Result<serde_json::Value, Error>) -> bool {
        self.phase = Phase::Result;
        self.scroll.set(0);
        match result {
            Ok(value) => {
                if self.comment {
                    self.result = format!(
                        "{}: {}\nRegistered / 登録済み; Issue List unchanged",
                        if value["resource"] == "issue" {
                            "Updated"
                        } else {
                            "Created"
                        },
                        self.update_key.as_deref().unwrap()
                    );
                    return false;
                }
                if self.update_key.is_some() {
                    self.result = format!(
                        "Updated: {}\n{}\nDetail updated; Issue List unchanged",
                        value["issueKey"].as_str().unwrap(),
                        value["url"].as_str().unwrap()
                    );
                    return false;
                }
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
                    Some(crate::WriteOutcome::Updated) => {
                        "Updated; response processing failed. Do not resend."
                    }
                    Some(crate::WriteOutcome::NotUpdated) => "NotUpdated: issue not updated.",
                    Some(crate::WriteOutcome::Unknown) if self.update_key.is_some() => {
                        "Unknown: update result unknown. Do not resend."
                    }
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
    pub(super) fn refreshing_comments(&mut self) {
        self.refreshing = true;
        self.result.push_str(if self.comment {
            "\nRegistered; refreshing comments / 登録済み・コメント表示更新中"
        } else {
            "\nUpdated; refreshing comments / 更新済み・コメント表示更新中"
        });
    }
    pub(super) fn comments_refreshed(&mut self, result: Result<(), String>) {
        self.refreshing = false;
        if let Some(prefix) = self.result.strip_suffix(if self.comment {
            "\nRegistered; refreshing comments / 登録済み・コメント表示更新中"
        } else {
            "\nUpdated; refreshing comments / 更新済み・コメント表示更新中"
        }) {
            self.result = prefix.into();
        }
        if self.comment {
            self.result.push_str(&match result {
                Ok(()) => "\nComment display refreshed / 登録済み・コメント表示更新完了".into(),
                Err(error) => format!("\n登録済み・コメント表示更新失敗\nOld display retained / 旧表示保持. Do not resend / 再送不要.\nComment R: refresh display\n{error}"),
            });
            return;
        }
        self.result.push_str(&match result {
            Ok(()) => "\nUpdated; Comment display refreshed / 更新済み・コメント表示更新完了".into(),
            Err(error) => format!(
                "\nUpdated; Comment display refresh failed / 更新済み・コメント表示更新失敗\nOld display retained / 旧表示保持. Do not resend / 再送不要.\nComment R: refresh display\n{error}"
            ),
        });
    }
    fn choice_field(&self) -> Option<usize> {
        match self.field {
            1..=3 => Some(self.field - 1),
            6 => Some(3),
            8 | 9 if self.update_key.is_some() => Some(self.field - 4),
            _ => None,
        }
    }
    fn changed(&self) -> bool {
        let mut current = self.values();
        let mut initial = self.initial.clone();
        for values in [&mut current, &mut initial] {
            values.choices[3].sort_by(|a, b| a.id.cmp(&b.id));
        }
        current != initial
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
            Phase::Sending | Phase::Preparing => return Action::None,
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
                8 | 9 => self.field - 4,
                _ => self.field - 1,
            };
            let nullable = field == 2 || field == 5;
            let length = self.options[field].len() + usize::from(nullable);
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
                        } else if nullable && popup.index == 0 {
                            self.choices[field].clear();
                            self.popup = None;
                        } else if let Some(choice) = self.options[field]
                            .get(popup.index - usize::from(nullable))
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
        if matches!(self.field, 0 | 4 | 5)
            && tui_input::backend::crossterm::to_input_request(event).is_some()
        {
            self.input_event(event, area);
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
            if let Some(field) = target.filter(|field| !self.comment || matches!(field, 3 | 7)) {
                self.field = field;
                self.notice.clear();
                return Action::None;
            }
        }
        match key.code {
            KeyCode::Esc if plain => return self.cancel(),
            KeyCode::Tab if plain => {
                let fields = self.fields();
                self.field = fields
                    [(fields.iter().position(|f| *f == self.field).unwrap() + 1) % fields.len()];
            }
            KeyCode::BackTab if key.modifiers == KeyModifiers::SHIFT || plain => {
                let fields = self.fields();
                self.field = fields[(fields.iter().position(|f| *f == self.field).unwrap()
                    + fields.len()
                    - 1)
                    % fields.len()];
            }
            KeyCode::Delete | KeyCode::Backspace if plain && self.choice_field().is_some() => {
                let index = self.choice_field().unwrap();
                if self.update_key.is_some() && matches!(index, 0 | 1 | 4) {
                    self.notice = "This field requires a selected value".into();
                } else {
                    self.choices[index].clear();
                }
            }
            KeyCode::Delete | KeyCode::Backspace if plain && self.field == 7 => self.body.clear(),
            _ if activate && self.choice_field().is_some() => {
                self.popup = Some(Popup {
                    index: 0,
                    horizontal: 0,
                })
            }
            _ if activate && self.field == 7 => return Action::Editor,
            _ if activate && self.field == self.field_count() - 2 => {
                if self.update_key.is_some() {
                    self.phase = Phase::Preparing;
                } else if self.title.value().trim().is_empty() {
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
            _ if activate && self.field == self.field_count() - 1 => return self.cancel(),
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
        if let Some(prepared) = &self.prepared {
            let mut text = format!("{}\nProject ID: {}", self.identity, self.project.id);
            for (parameter, value) in prepared.changes() {
                let label = match *parameter {
                    "summary" => "Title",
                    "description" | "content" | "comment" => "Body",
                    "issueTypeId" => "Issue type",
                    "priorityId" => "Priority",
                    "assigneeId" => "Assignee",
                    "statusId" => "Status",
                    "resolutionId" => "Resolution",
                    "startDate" => "Start date",
                    "dueDate" => "Due date",
                    _ => "Milestones",
                };
                let choice = match *parameter {
                    "issueTypeId" => Some(0),
                    "priorityId" => Some(1),
                    "assigneeId" => Some(2),
                    "milestoneId[]" => Some(3),
                    "statusId" => Some(4),
                    "resolutionId" => Some(5),
                    _ => None,
                };
                let display = choice
                    .and_then(|index| self.choices[index].iter().find(|c| c.id == *value))
                    .map(Choice::display);
                text.push_str(&format!(
                    "\n{label}: {}",
                    if value.is_empty() {
                        "(cleared)"
                    } else {
                        display.as_deref().unwrap_or(value)
                    }
                ));
            }
            text.push_str("\n[End of changes]");
            return text;
        }
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
            Phase::Confirm if self.comment => "Confirm comment (no write yet)",
            Phase::Preparing if self.comment => "Preparing comment (no write yet)",
            Phase::Result if self.comment => "Comment result",
            Phase::Loading | Phase::LoadFailed | Phase::Form if self.comment => "Add comment",
            Phase::Confirm if self.update_key.is_some() => "Confirm update (no PATCH yet)",
            Phase::Confirm => "Confirm create (no POST yet)",
            Phase::Preparing => "Preparing update (no PATCH yet)",
            Phase::Sending => "Sending - Ctrl+C: interrupt",
            Phase::Result if self.update_key.is_some() => "Update result",
            Phase::Result => "Create result",
            Phase::Discard => "Discard changes?",
            _ if self.update_key.is_some() => "Update issue",
            _ => "Create issue",
        };
        frame.render_widget(Clear, area);
        frame.render_widget(Block::default().borders(Borders::ALL).title(title), area);
        let inner = Rect::new(area.x + 1, area.y + 1, area.width - 2, area.height - 2);
        let safe = |text: &str| {
            crate::tui_text::display(Text::from(safety::text(text, secrets)), secrets).to_string()
        };
        if self.phase == Phase::Preparing {
            frame.render_widget(
                Paragraph::new("Validating changes... Ctrl+C: interrupt"),
                inner,
            );
            return;
        }
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
                        if self.update_key.is_some() {
                            "Loading issue / candidates..."
                        } else {
                            "Loading create candidates..."
                        }
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
                if self.comment {
                    "j/k PgUp/Dn Home/End: read all\nEnter/Esc: Comment"
                } else if self.update_key.is_some() {
                    "j/k PgUp/Dn Home/End: read all\nEnter/Esc: Detail"
                } else {
                    "j/k PgUp/Dn Home/End: read all\nEnter/Esc: Issue List"
                }
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
        let mut labels = vec![
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
        if self.update_key.is_some() {
            labels.splice(8..8, ["Status", "Resolution"]);
        }
        let visible = inner.height.saturating_sub(4) as usize;
        let fields = self.fields();
        let top = fields
            .iter()
            .position(|f| *f == self.field)
            .unwrap()
            .saturating_sub(visible.saturating_sub(1));
        for (row, index) in fields.into_iter().skip(top).take(visible).enumerate() {
            let value = match index {
                0 => String::new(),
                1..=3 => self.selection_text(index - 1),
                4 | 5 => String::new(),
                6 => self.selection_text(3),
                8 | 9 if self.update_key.is_some() => self.selection_text(index - 4),
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
            if field == 2 || field == 5 {
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
    prepared: Option<Prepared>,
}
impl Submission {
    pub(super) async fn run(self, service: Service) -> Result<serde_json::Value, Error> {
        if let Some(prepared) = self.prepared {
            match prepared {
                Prepared::Issue(p) => service.send_issue_update(p).await,
                Prepared::Comment(p) => service.send_comment_add(p).await,
            }
        } else {
            self.values.send(&self.project, &service).await
        }
    }
}
pub(super) struct Preparation {
    values: Values,
    initial: Values,
    issue: serde_json::Value,
    key: String,
    comment: bool,
}
impl Preparation {
    pub(super) async fn run(self, service: Service) -> Result<Prepared, Error> {
        let v = &self.values;
        let i = &self.initial;
        let changed = |index: usize| v.choices[index] != i.choices[index];
        let selected = |index: usize| {
            if changed(index) {
                v.choices[index].first().map(Choice::selection)
            } else {
                None
            }
        };
        let clear = |index: usize| changed(index) && v.choices[index].is_empty();
        if self.comment {
            return service
                .prepare_comment_add(
                    &self.key,
                    CommentAdd {
                        body: Some(&v.body),
                        status: selected(4),
                        assignee: selected(2),
                        resolution: selected(5),
                        clear_assignee: clear(2),
                        clear_resolution: clear(5),
                    },
                    Some(&self.issue),
                )
                .await
                .map(Prepared::Comment);
        }
        service
            .prepare_issue_update(
                &self.key,
                IssueUpdate {
                    title: (v.title != i.title).then_some(v.title.as_str()),
                    body: (v.body != i.body).then_some(v.body.as_str()),
                    issue_type: selected(0),
                    priority: selected(1),
                    assignee: selected(2),
                    status: selected(4),
                    resolution: selected(5),
                    clear_assignee: clear(2),
                    clear_resolution: clear(5),
                    milestone: if changed(3) {
                        v.choices[3].iter().map(Choice::selection).collect()
                    } else {
                        vec![]
                    },
                    clear_milestones: clear(3),
                    start_date: (v.start != i.start && !v.start.is_empty())
                        .then_some(v.start.as_str()),
                    due_date: (v.due != i.due && !v.due.is_empty()).then_some(v.due.as_str()),
                    clear_start_date: v.start != i.start && v.start.is_empty(),
                    clear_due_date: v.due != i.due && v.due.is_empty(),
                },
                Some(&self.issue),
            )
            .await
            .map(Prepared::Issue)
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
pub(super) fn wrapped_lines(content: &str, width: u16, secrets: &[&str]) -> Vec<String> {
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
