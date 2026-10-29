use super::{
    Focus,
    comment::{Comment, Direction, Entry},
    detail::{self, Detail},
    search::{self, Conditions, Dialog},
};
use crate::{
    Error,
    api::Api,
    config::Config,
    safety,
    service::{Candidate, CommentOrder, ResolvedIssueSearch, Service},
};
use crossterm::event::KeyCode;
use serde_json::{Number, Value};
use std::{process::Stdio, sync::Arc, time::Instant};

#[derive(Clone)]
pub(super) struct Project {
    pub(super) id: String,
    key: String,
    label: String,
}
#[derive(Default)]
struct Position {
    selected: usize,
    scroll: usize,
}
impl Position {
    fn move_by(&mut self, down: bool, length: usize, rows: usize) {
        self.selected = if down {
            (self.selected + 1).min(length.saturating_sub(1))
        } else {
            self.selected.saturating_sub(1)
        };
        self.scroll = self.start(rows);
    }
    fn start(&self, rows: usize) -> usize {
        self.scroll
            .min(self.selected)
            .max(self.selected.saturating_sub(rows.saturating_sub(1)))
    }
}
struct Search {
    project: Option<Project>,
    conditions: Conditions,
    resolved: ResolvedIssueSearch,
    total: Number,
    pages: Vec<Page>,
    current: usize,
}
struct Page {
    items: Vec<Value>,
    offset: usize,
    position: Position,
}
impl Search {
    fn header_rows(&self) -> usize {
        if self.project.is_none() { 4 } else { 2 }
    }
    fn next_offset(&self) -> Option<usize> {
        let page = &self.pages[self.current];
        if page.items.is_empty() {
            return None;
        }
        let offset = page.offset.checked_add(page.items.len())?;
        // Compare decimal integers without narrowing arbitrary-precision API totals.
        let decimal = offset.to_string();
        let total = self.total.to_string();
        ((decimal.len(), decimal.as_str()) < (total.len(), total.as_str())).then_some(offset)
    }
}
#[derive(Clone, Copy, PartialEq)]
enum Target {
    Projects,
    Issues,
    Detail,
    Comments,
    Browser,
    Candidates,
}
#[derive(Clone, Copy)]
pub(super) struct RequestId(Target, u64);

enum Operation {
    Comments {
        key: String,
        direction: Direction,
        boundary: Option<String>,
    },
    Candidates {
        project: String,
        field: usize,
    },
    Detail {
        key: String,
    },
    Browser {
        key: String,
    },
    Initial,
    Projects,
    Search {
        project: Option<Project>,
        conditions: Option<Conditions>,
        resolved: Option<ResolvedIssueSearch>,
        apply: bool,
    },
    Page {
        resolved: ResolvedIssueSearch,
        offset: usize,
    },
}
pub(super) struct Request {
    generation: u64,
    target: Target,
    operation: Operation,
}
enum Data {
    Comments(String, Direction, Vec<Entry>),
    Candidates(Value),
    Detail(String, String),
    Browser,
    Initial(Value, Value),
    Projects(Value),
    Search(
        Option<Project>,
        Conditions,
        ResolvedIssueSearch,
        Value,
        bool,
    ),
    Page(usize, Value),
}
pub(super) struct Response {
    generation: u64,
    target: Target,
    initial: bool,
    result: Result<Data, Error>,
}
impl Request {
    pub(super) fn id(&self) -> RequestId {
        RequestId(self.target, self.generation)
    }
    pub(super) async fn run(self, config: Arc<Config>) -> Response {
        let initial = matches!(self.operation, Operation::Initial);
        let result = async {
            match self.operation {
                Operation::Comments {
                    key,
                    direction,
                    boundary,
                } => {
                    if !crate::service::issue_key(&key) {
                        return Err(Error::new("invalid_response", "Invalid selected issue key"));
                    }
                    let service = Service::new(&config)?;
                    let (min, max, order) = match direction {
                        Direction::Latest => (None, None, CommentOrder::Desc),
                        Direction::Oldest => (None, None, CommentOrder::Asc),
                        Direction::Older => (None, boundary.as_deref(), CommentOrder::Desc),
                        Direction::Newer => (boundary.as_deref(), None, CommentOrder::Asc),
                    };
                    let page = service.comment_page(&key, 20, min, max, order).await?;
                    let items = page["items"].as_array().unwrap();
                    let rule = if items.iter().any(|item| item["content"].is_string()) {
                        service.issue_text_formatting_rule(&key).await?
                    } else {
                        String::new()
                    };
                    let items = items
                        .iter()
                        .map(|item| {
                            let preview = crate::comment::preview(item, &rule, &config)?;
                            Ok(Entry {
                                id: item["id"].to_string(),
                                preview,
                            })
                        })
                        .collect::<Result<_, Error>>()?;
                    Ok(Data::Comments(key, direction, items))
                }
                Operation::Candidates { project, field } => {
                    let candidate = match field {
                        0 => Candidate::Status { project: &project },
                        1 => Candidate::Assignee { project: &project },
                        _ => Candidate::Milestone { project: &project },
                    };
                    Ok(Data::Candidates(
                        Service::new(&config)?.candidates(candidate).await?,
                    ))
                }
                Operation::Detail { key } => {
                    if !crate::service::issue_key(&key) {
                        return Err(Error::new("invalid_response", "Invalid selected issue key"));
                    }
                    let service = Service::new(&config)?;
                    let issue = service.issue_view(&key).await?;
                    if issue["issueKey"].as_str() != Some(&key) {
                        return Err(Error::new(
                            "invalid_response",
                            "Issue response does not match selected key",
                        ));
                    }
                    let rule = service
                        .project_text_formatting_rule(issue["projectId"].as_number().unwrap())
                        .await?;
                    Ok(Data::Detail(key, detail::display(&issue, &rule, &config)))
                }
                Operation::Browser { key } => {
                    if !crate::service::issue_key(&key) {
                        return Err(Error::new("invalid_response", "Invalid selected issue key"));
                    }
                    let url = crate::api::issue_url(&config.origin, &key);
                    if url.contains(&config.api_key) {
                        return Err(Error::new(
                            "input",
                            "Cannot open a URL containing the API key",
                        ));
                    }
                    let status = tokio::process::Command::new("open")
                        .arg(url)
                        .stdin(Stdio::null())
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .kill_on_drop(true)
                        .status()
                        .await
                        .map_err(|_| Error::new("output", "Cannot launch browser"))?;
                    if !status.success() {
                        return Err(Error::new("output", "Browser launch failed"));
                    }
                    Ok(Data::Browser)
                }
                Operation::Initial => {
                    let api = Api::new(&config)?;
                    let space = api.space().await.map_err(|mut error| {
                        error.message =
                            format!("Initial space retrieval failed: {}", error.message);
                        error
                    })?;
                    let projects = api.project_list().await.map_err(|mut error| {
                        error.message =
                            format!("Initial projects retrieval failed: {}", error.message);
                        error
                    })?;
                    Ok(Data::Initial(space, projects))
                }
                Operation::Projects => Ok(Data::Projects(Api::new(&config)?.project_list().await?)),
                Operation::Page { resolved, offset } => Ok(Data::Page(
                    offset,
                    Service::new(&config)?
                        .search_page(&resolved, &offset.to_string())
                        .await?,
                )),
                Operation::Search {
                    project,
                    conditions,
                    resolved,
                    apply,
                } => {
                    let service = Service::new(&config)?;
                    let mut conditions =
                        conditions.unwrap_or_else(|| Conditions::defaults(&config.settings));
                    let resolved = match resolved {
                        Some(resolved) => resolved,
                        None => {
                            conditions
                                .resolve(
                                    &service,
                                    project.as_ref().map_or("all", |p| p.id.as_str()),
                                )
                                .await?
                        }
                    };
                    let page = service.search_resolved(&resolved).await?;
                    Ok(Data::Search(project, conditions, resolved, page, apply))
                }
            }
        }
        .await;
        Response {
            generation: self.generation,
            target: self.target,
            initial,
            result,
        }
    }
}

#[derive(Default)]
pub(super) struct Browse {
    pub(super) space: Option<String>,
    projects: Vec<Project>,
    projects_loaded: bool,
    position: Position,
    search: Option<Search>,
    confirmed_project: Option<Project>,
    defaults: Conditions,
    pub(super) dialog: Option<Dialog>,
    pub(super) focus_issue: bool,
    generation: u64,
    projects_busy: Option<u64>,
    issues_busy: Option<u64>,
    detail_busy: Option<u64>,
    comments_busy: Option<u64>,
    browser_busy: Option<u64>,
    candidates_busy: Option<u64>,
    pub(super) detail: Detail,
    pub(super) comment: Comment,
    pub(super) requests: Vec<Request>,
    notice: String,
    pub(super) error_notice: Option<String>,
    pub(super) secret: String,
}
impl Browse {
    pub(super) fn is_current(&self, id: RequestId) -> bool {
        (match id.0 {
            Target::Projects => self.projects_busy,
            Target::Issues => self.issues_busy,
            Target::Detail => self.detail_busy,
            Target::Comments => self.comments_busy,
            Target::Browser => self.browser_busy,
            Target::Candidates => self.candidates_busy,
        }) == Some(id.1)
    }
    pub(super) fn start(&mut self, config: &Config) {
        self.secret = config.api_key.clone();
        self.defaults = Conditions::defaults(&config.settings);
        self.request(Target::Projects, Operation::Initial);
    }
    fn request(&mut self, target: Target, operation: Operation) {
        self.generation += 1;
        *self.busy(target) = Some(self.generation);
        if target == Target::Detail {
            self.detail.notice.clear();
        } else if target == Target::Comments {
            self.comment.document.notice.clear();
        } else {
            self.notice.clear();
        }
        self.requests.push(Request {
            generation: self.generation,
            target,
            operation,
        });
    }
    fn busy(&mut self, target: Target) -> &mut Option<u64> {
        match target {
            Target::Projects => &mut self.projects_busy,
            Target::Issues => &mut self.issues_busy,
            Target::Detail => &mut self.detail_busy,
            Target::Comments => &mut self.comments_busy,
            Target::Browser => &mut self.browser_busy,
            Target::Candidates => &mut self.candidates_busy,
        }
    }
    pub(super) fn complete(
        &mut self,
        response: Response,
        configured_project: Option<&str>,
    ) -> Result<(), Error> {
        if *self.busy(response.target) != Some(response.generation) {
            return Ok(());
        }
        *self.busy(response.target) = None;
        let data = match response.result {
            Ok(data) => data,
            Err(error) if response.initial => return Err(error),
            Err(error) => {
                let notice = safety::text(&format!("Failed: {}", error.message), &[&self.secret]);
                if response.target == Target::Candidates
                    && let Some(candidates) =
                        self.dialog.as_mut().and_then(|d| d.candidates.as_mut())
                {
                    candidates.failed = true;
                }
                if response.target == Target::Issues
                    && let Some(dialog) = &mut self.dialog
                {
                    dialog.applying = false;
                }
                if response.target == Target::Detail {
                    self.detail.status = "Preview failed";
                }
                if response.target == Target::Comments {
                    self.comment.document.status = "Comment preview failed";
                }
                self.error_notice = Some(notice);
                return Ok(());
            }
        };
        let reset_detail = matches!(data, Data::Search(..));
        match data {
            Data::Comments(key, direction, items) => {
                if self.comment.document.key.as_ref() == Some(&key) {
                    self.comment.replace(items, direction);
                }
            }
            Data::Candidates(items) => {
                if let Some(dialog) = &mut self.dialog {
                    let choices = items
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|item| search::Choice {
                            id: Some(item["id"].to_string()),
                            label: item["name"].as_str().unwrap().into(),
                        })
                        .collect();
                    dialog.load_candidates(choices);
                }
            }
            Data::Detail(key, text) => {
                if self.detail.key.as_ref() == Some(&key) {
                    self.detail.replace(text.into());
                }
            }
            Data::Browser => {}
            Data::Initial(space, projects) => {
                self.space = Some(safety::text(
                    space["name"].as_str().unwrap(),
                    &[&self.secret],
                ));
                self.set_projects(projects);
                if let Some(index) = self.projects.iter().position(|p| {
                    configured_project
                        .is_some_and(|name| name.trim_start_matches('0') == p.id || name == p.key)
                }) {
                    self.position.selected = index;
                    self.confirm();
                }
            }
            Data::Projects(projects) => self.set_projects(projects),
            Data::Search(project, conditions, resolved, page, apply) => {
                if apply {
                    self.dialog = None;
                    self.focus_issue = true;
                }
                self.search = Some(Search {
                    project,
                    conditions,
                    resolved,
                    total: page["pagination"]["total"].as_number().unwrap().clone(),
                    pages: vec![Page {
                        items: page["items"].as_array().unwrap().clone(),
                        offset: 0,
                        position: Position::default(),
                    }],
                    current: 0,
                })
            }
            Data::Page(offset, items) => {
                if let Some(search) = &mut self.search {
                    search.pages.push(Page {
                        offset,
                        items: items.as_array().unwrap().clone(),
                        position: Position::default(),
                    });
                    search.current = search.pages.len() - 1;
                }
            }
        }
        self.sync_selection(reset_detail);
        Ok(())
    }
    fn selected_key(&self) -> Option<String> {
        let search = self.search.as_ref()?;
        let page = &search.pages[search.current];
        page.items.get(page.position.selected)?["issueKey"]
            .as_str()
            .map(str::to_owned)
    }
    fn sync_selection(&mut self, force: bool) {
        let key = self.selected_key();
        if force || self.detail.key != key {
            self.detail_busy = None;
            self.comments_busy = None;
            self.browser_busy = None;
            self.comment.select(key.clone());
            self.detail.select(key);
        }
    }
    pub(super) fn tick(&mut self) {
        if self
            .comment
            .document
            .due
            .is_some_and(|due| Instant::now() >= due)
        {
            self.comment.document.due = None;
            if let Some(key) = self.comment.document.key.clone() {
                self.request(
                    Target::Comments,
                    Operation::Comments {
                        key,
                        direction: Direction::Latest,
                        boundary: None,
                    },
                );
            }
        }
        if self.detail.due.is_some_and(|due| Instant::now() >= due) {
            self.detail.due = None;
            if let Some(key) = self.detail.key.clone() {
                self.request(Target::Detail, Operation::Detail { key });
            }
        }
    }
    pub(super) fn open_browser(&mut self) {
        if let Some(key) = self.selected_key() {
            self.request(Target::Browser, Operation::Browser { key });
        }
    }
    pub(super) fn cancel_previews(&mut self) {
        if self.comments_busy.is_some() || self.comment.document.due.is_some() {
            self.comment.document.status = "Comment preview cancelled";
            self.comment.document.notice = "Cancelled; display retained".into();
        }
        self.comments_busy = None;
        self.comment.document.due = None;
        if self.detail_busy.is_some() || self.detail.due.is_some() {
            self.detail.status = "Preview cancelled";
            self.detail.notice = "Cancelled; display retained".into();
        }
        self.detail_busy = None;
        self.detail.due = None;
    }
    pub(super) fn comment_status(&self) -> String {
        let status = if self.comments_busy.is_some() {
            "Loading... Esc: cancel"
        } else {
            ""
        };
        format!("{status} {}", self.comment.document.notice)
    }
    pub(super) fn detail_status(&self) -> String {
        let status = if self.detail_busy.is_some() {
            "Loading... Esc: cancel"
        } else {
            ""
        };
        format!("{status} {}", self.detail.notice)
    }
    fn set_projects(&mut self, projects: Value) {
        self.projects_loaded = true;
        let selected = self
            .projects
            .get(self.position.selected)
            .map(|p| p.id.clone());
        self.projects = projects
            .as_array()
            .unwrap()
            .iter()
            .map(|p| Project {
                id: p["id"].to_string(),
                key: p["projectKey"].as_str().unwrap().into(),
                label: safety::text(
                    &format!(
                        "{}  {}",
                        p["projectKey"].as_str().unwrap(),
                        p["name"].as_str().unwrap()
                    ),
                    &[&self.secret],
                ),
            })
            .collect();
        self.position.selected = selected
            .and_then(|id| self.projects.iter().position(|p| p.id == id))
            .unwrap_or_else(|| {
                self.position
                    .selected
                    .min(self.projects.len().saturating_sub(1))
            });
        self.position.scroll = self.position.scroll.min(self.position.selected);
    }
    fn confirm(&mut self) {
        if let Some(project) = self.projects.get(self.position.selected) {
            self.confirmed_project = Some(project.clone());
            if self
                .search
                .as_ref()
                .is_some_and(|search| search.project.as_ref().is_some_and(|p| p.id == project.id))
            {
                self.issues_busy = None;
                self.notice.clear();
                return;
            }
            self.request(
                Target::Issues,
                Operation::Search {
                    project: Some(project.clone()),
                    conditions: None,
                    resolved: None,
                    apply: false,
                },
            );
        }
    }
    pub(super) fn key(&mut self, code: KeyCode, focus: Focus, rows: usize) {
        if !self.projects_loaded {
            return;
        }
        match (focus, code) {
            (_, KeyCode::Esc) if self.projects_busy.is_some() || self.issues_busy.is_some() => {
                self.projects_busy = None;
                self.issues_busy = None;
                self.cancel_previews();
                self.browser_busy = None;
                self.requests.clear();
                self.notice = "Cancelled; display retained".into();
            }
            (Focus::Comment, KeyCode::Char('n' | 'p' | 'g' | 'G' | 'R')) => {
                if matches!(code, KeyCode::Char('n' | 'p')) && self.comment.is_empty() {
                    return;
                }
                self.comments_busy = None;
                self.comment.document.due = None;
                if let Some(key) = self.comment.document.key.clone()
                    && let Some((direction, boundary)) = self.comment.navigate(code)
                {
                    self.request(
                        Target::Comments,
                        Operation::Comments {
                            key,
                            direction,
                            boundary,
                        },
                    );
                }
            }
            (Focus::Detail, KeyCode::Char('R')) => {
                if let Some(key) = self.detail.key.clone() {
                    self.detail.due = None;
                    self.request(Target::Detail, Operation::Detail { key });
                }
            }
            (_, KeyCode::Esc) => self.cancel_previews(),
            (Focus::Project, KeyCode::Down | KeyCode::Char('j')) => {
                self.position.move_by(true, self.projects.len(), rows)
            }
            (Focus::Project, KeyCode::Up | KeyCode::Char('k')) => {
                self.position.move_by(false, self.projects.len(), rows)
            }
            (Focus::Project, KeyCode::Enter) => self.confirm(),
            (Focus::Project, KeyCode::Char('R')) => {
                self.request(Target::Projects, Operation::Projects)
            }
            (Focus::Issue, KeyCode::Down | KeyCode::Char('j')) => {
                if let Some(search) = &mut self.search {
                    let visible_rows = rows.saturating_sub(search.header_rows());
                    let page = &mut search.pages[search.current];
                    page.position.move_by(true, page.items.len(), visible_rows);
                }
            }
            (Focus::Issue, KeyCode::Up | KeyCode::Char('k')) => {
                if let Some(search) = &mut self.search {
                    let visible_rows = rows.saturating_sub(search.header_rows());
                    let page = &mut search.pages[search.current];
                    page.position.move_by(false, page.items.len(), visible_rows);
                }
            }
            (Focus::Issue, KeyCode::Char('p')) => {
                if let Some(search) = &mut self.search {
                    self.issues_busy = None;
                    self.notice.clear();
                    search.current = search.current.saturating_sub(1);
                }
            }
            (Focus::Issue, KeyCode::Char('n')) => {
                if let Some(search) = &mut self.search {
                    if search.current + 1 < search.pages.len() {
                        self.issues_busy = None;
                        self.notice.clear();
                        search.current += 1;
                    } else if let Some(offset) = search.next_offset() {
                        let resolved = search.resolved.clone();
                        self.request(Target::Issues, Operation::Page { resolved, offset });
                    }
                }
            }
            (Focus::Issue, KeyCode::Char('c'))
                if self.search.as_ref().is_some_and(|s| s.project.is_none()) =>
            {
                self.error_notice = Some(
                    "Select a concrete project: h/l or Tab -> Project, then Enter for normal search / create target".into(),
                );
            }
            (Focus::Issue, KeyCode::Char('R')) => {
                if let Some(search) = &self.search {
                    self.request(
                        Target::Issues,
                        Operation::Search {
                            project: search.project.clone(),
                            conditions: Some(search.conditions.clone()),
                            resolved: Some(search.resolved.clone()),
                            apply: false,
                        },
                    );
                }
            }
            _ => {}
        }
        self.sync_selection(false);
    }
    pub(super) fn open_search(&mut self, cross: bool) {
        if !self.projects_loaded {
            return;
        }
        let (project, conditions) = if cross {
            (
                None,
                self.search
                    .as_ref()
                    .filter(|s| s.project.is_none())
                    .map(|s| s.conditions.clone())
                    .unwrap_or_default(),
            )
        } else if let Some(search) = &self.search {
            (search.project.clone(), search.conditions.clone())
        } else if let Some(project) = &self.confirmed_project {
            (Some(project.clone()), self.defaults.clone())
        } else {
            return;
        };
        self.issues_busy = None;
        self.candidates_busy = None;
        self.dialog = Some(Dialog::new(project, conditions));
    }
    pub(super) fn dialog_event(
        &mut self,
        event: &crossterm::event::Event,
        area: ratatui::layout::Rect,
    ) {
        let action = self.dialog.as_mut().unwrap().event(event, area);
        match action {
            search::Action::None => {}
            search::Action::CancelCandidates => {
                self.candidates_busy = None;
            }
            search::Action::Candidates(field) => {
                let dialog = self.dialog.as_ref().unwrap();
                if let Some(project) = &dialog.project {
                    self.request(
                        Target::Candidates,
                        Operation::Candidates {
                            project: project.id.clone(),
                            field,
                        },
                    );
                }
            }
            search::Action::Close => {
                self.dialog = None;
                self.candidates_busy = None;
                self.issues_busy = None;
            }
            search::Action::Apply => {
                self.candidates_busy = None;
                let dialog = self.dialog.as_mut().unwrap();
                dialog.applying = true;
                let operation = Operation::Search {
                    project: dialog.project.clone(),
                    conditions: Some(dialog.conditions.clone()),
                    resolved: None,
                    apply: true,
                };
                self.request(Target::Issues, operation);
            }
        }
    }
    pub(super) fn content(&self, focus: Focus, rows: usize) -> String {
        let busy = match focus {
            Focus::Project if self.projects_loaded => self.projects_busy,
            Focus::Issue => self.issues_busy,
            _ => None,
        };
        let rows =
            rows.saturating_sub(usize::from(busy.is_some()) + usize::from(!self.notice.is_empty()));
        let mut content = match focus {
            Focus::Project if !self.projects_loaded => "Loading projects...".into(),
            Focus::Project => {
                let mut text = if self.projects.is_empty() {
                    "No projects\n".into()
                } else {
                    String::new()
                };
                if self.search.is_none() {
                    text.push_str("Select a project; Enter to confirm\n");
                }
                for (index, project) in self
                    .projects
                    .iter()
                    .enumerate()
                    .skip(self.position.start(rows.saturating_sub(1)))
                {
                    text.push_str(&format!(
                        "{} {}\n",
                        if index == self.position.selected {
                            ">"
                        } else {
                            " "
                        },
                        project.label
                    ));
                }
                text
            }
            Focus::Issue => match &self.search {
                None => "Select a project; Enter to confirm".into(),
                Some(search) => {
                    let page = &search.pages[search.current];
                    let mut text = format!(
                        "{} | total {} | page {}\n",
                        search.project.as_ref().map_or_else(
                            || "Cross-project".into(),
                            |p| format!("Project: {}", p.key)
                        ),
                        search.total,
                        search.current + 1
                    );
                    text.push_str(&format!("{}\n", search.conditions.summary(&[&self.secret])));
                    if search.project.is_none() {
                        text.push_str(
                            "h/l or Tab -> Project\nEnter: normal search / create target\n",
                        );
                    }
                    if page.items.is_empty() {
                        text.push_str("No issues");
                    }
                    for (index, item) in page.items.iter().enumerate().skip(
                        page.position
                            .start(rows.saturating_sub(search.header_rows())),
                    ) {
                        text.push_str(&safety::text(
                            &format!(
                                "{} {} {} [{}]\n",
                                if index == page.position.selected {
                                    ">"
                                } else {
                                    " "
                                },
                                item["issueKey"].as_str().unwrap(),
                                item["summary"].as_str().unwrap(),
                                item["status"]["name"].as_str().unwrap()
                            ),
                            &[&self.secret],
                        ));
                    }
                    text
                }
            },
            _ => "Not implemented".into(),
        };
        if busy.is_some() {
            content = format!("Loading... Esc: cancel\n{content}");
        }
        if !self.notice.is_empty() {
            content = format!("{}\n{content}", self.notice);
        }
        safety::text(&content, &[&self.secret])
    }
}
