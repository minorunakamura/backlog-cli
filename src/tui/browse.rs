use super::Focus;
use crate::{
    Error,
    api::Api,
    config::Config,
    safety,
    service::{IssueSearch, ResolvedIssueSearch, Service},
};
use crossterm::event::KeyCode;
use serde_json::{Number, Value};
use std::sync::Arc;

#[derive(Clone)]
struct Project {
    id: String,
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
    project: Project,
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
}
#[derive(Clone, Copy)]
pub(super) struct RequestId(Target, u64);

enum Operation {
    Initial,
    Projects,
    Search {
        project: Project,
        resolved: Option<ResolvedIssueSearch>,
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
    Initial(Value, Value),
    Projects(Value),
    Search(Project, ResolvedIssueSearch, Value),
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
                Operation::Search { project, resolved } => {
                    let service = Service::new(&config)?;
                    let resolved = match resolved {
                        Some(resolved) => resolved,
                        None => {
                            service
                                .resolve_search(
                                    &config.settings,
                                    IssueSearch {
                                        project: Some(&project.id),
                                        ..Default::default()
                                    },
                                )
                                .await?
                        }
                    };
                    let page = service.search_resolved(&resolved).await?;
                    Ok(Data::Search(project, resolved, page))
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
    generation: u64,
    projects_busy: Option<u64>,
    issues_busy: Option<u64>,
    pub(super) requests: Vec<Request>,
    notice: String,
    secret: String,
}
impl Browse {
    pub(super) fn is_current(&self, id: RequestId) -> bool {
        (match id.0 {
            Target::Projects => self.projects_busy,
            Target::Issues => self.issues_busy,
        }) == Some(id.1)
    }
    pub(super) fn start(&mut self, secret: &str) {
        self.secret = secret.into();
        self.request(Target::Projects, Operation::Initial);
    }
    fn request(&mut self, target: Target, operation: Operation) {
        self.generation += 1;
        *self.busy(target) = Some(self.generation);
        self.notice.clear();
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
                self.notice = safety::text(&format!("Failed: {}", error.message), &[&self.secret]);
                return Ok(());
            }
        };
        match data {
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
            Data::Search(project, resolved, page) => {
                self.search = Some(Search {
                    project,
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
        Ok(())
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
            if self
                .search
                .as_ref()
                .is_some_and(|search| search.project.id == project.id)
            {
                self.issues_busy = None;
                self.notice.clear();
                return;
            }
            self.request(
                Target::Issues,
                Operation::Search {
                    project: project.clone(),
                    resolved: None,
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
                self.requests.clear();
                self.notice = "Cancelled; display retained".into();
            }
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
                    let page = &mut search.pages[search.current];
                    page.position
                        .move_by(true, page.items.len(), rows.saturating_sub(1));
                }
            }
            (Focus::Issue, KeyCode::Up | KeyCode::Char('k')) => {
                if let Some(search) = &mut self.search {
                    let page = &mut search.pages[search.current];
                    page.position
                        .move_by(false, page.items.len(), rows.saturating_sub(1));
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
            (Focus::Issue, KeyCode::Char('R')) => {
                if let Some(search) = &self.search {
                    self.request(
                        Target::Issues,
                        Operation::Search {
                            project: search.project.clone(),
                            resolved: Some(search.resolved.clone()),
                        },
                    );
                }
            }
            _ => {}
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
                        "Project: {} | total {} | page {}\n",
                        search.project.key,
                        search.total,
                        search.current + 1
                    );
                    if page.items.is_empty() {
                        text.push_str("No issues");
                    }
                    for (index, item) in page
                        .items
                        .iter()
                        .enumerate()
                        .skip(page.position.start(rows.saturating_sub(1)))
                    {
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
