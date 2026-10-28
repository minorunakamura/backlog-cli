use crate::{
    CreateOutcome, Error, WriteOutcome,
    api::Api,
    config::{Config, Settings},
};
use serde_json::{Number, Value};

#[derive(Clone, Copy)]
pub enum Candidate<'a> {
    IssueType { project: &'a str },
    Status { project: &'a str },
    Assignee { project: &'a str },
    Milestone { project: &'a str },
    Priority,
    Resolution,
}
/// Keep numeric names distinct from explicit IDs; neither is inferred from the text.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Selection<'a> {
    Name(&'a str),
    Id(&'a str),
}

pub struct IssueSearch<'a> {
    pub project: Option<&'a str>,
    pub keyword: Option<&'a str>,
    pub status: Vec<Selection<'a>>,
    pub assignee: Vec<Selection<'a>>,
    pub milestone: Vec<Selection<'a>>,
    pub mine: bool,
    pub limit: u8,
    pub offset: &'a str,
}
impl Default for IssueSearch<'_> {
    fn default() -> Self {
        Self {
            project: None,
            keyword: None,
            status: vec![],
            assignee: vec![],
            milestone: vec![],
            mine: false,
            limit: 20,
            offset: "0",
        }
    }
}

/// Validated, resolved conditions reusable for a search refresh or list-only page.
#[derive(Clone)]
pub struct ResolvedIssueSearch {
    query: Vec<(&'static str, String)>,
    limit: u8,
    offset: Number,
}

#[derive(Default)]
pub struct IssueCreate<'a> {
    pub project: Option<&'a str>,
    pub title: &'a str,
    pub body: Option<&'a str>,
    pub issue_type: Option<Selection<'a>>,
    pub priority: Option<Selection<'a>>,
    pub milestone: Vec<Selection<'a>>,
    pub assignee: Option<Selection<'a>>,
    pub clear_assignee: bool,
    pub start_date: Option<&'a str>,
    pub due_date: Option<&'a str>,
}

#[derive(Default)]
pub struct IssueUpdate<'a> {
    pub title: Option<&'a str>,
    pub body: Option<&'a str>,
    pub issue_type: Option<Selection<'a>>,
    pub priority: Option<Selection<'a>>,
    pub status: Option<Selection<'a>>,
    pub assignee: Option<Selection<'a>>,
    pub resolution: Option<Selection<'a>>,
    pub milestone: Vec<Selection<'a>>,
    pub start_date: Option<&'a str>,
    pub due_date: Option<&'a str>,
    pub clear_assignee: bool,
    pub clear_resolution: bool,
    pub clear_start_date: bool,
    pub clear_due_date: bool,
    pub clear_milestones: bool,
}

#[derive(Default)]
pub struct CommentAdd<'a> {
    pub body: Option<&'a str>,
    pub status: Option<Selection<'a>>,
    pub assignee: Option<Selection<'a>>,
    pub resolution: Option<Selection<'a>>,
    pub clear_assignee: bool,
    pub clear_resolution: bool,
}

pub struct Service {
    api: Api,
}
impl Service {
    pub fn new(config: &Config) -> Result<Self, Error> {
        Ok(Self {
            api: Api::new(config)?,
        })
    }
    pub async fn comment_update(
        &self,
        issue: &str,
        comment_id: &str,
        body: &str,
    ) -> Result<Value, Error> {
        self.update_comment_inner(issue, comment_id, body)
            .await
            .map_err(|error| error.with_write_outcome("comment.update", WriteOutcome::NotUpdated))
    }
    async fn update_comment_inner(
        &self,
        issue: &str,
        comment_id: &str,
        body: &str,
    ) -> Result<Value, Error> {
        if body.is_empty() {
            return Err(Error::new("input", "Comment body must not be empty"));
        }
        let latest = self.comment_view(issue, comment_id).await?;
        let owner = latest["createdUser"]["id"]
            .as_number()
            .filter(|id| positive_integer(&id.to_string()))
            .ok_or_else(|| invalid_response("Comment response has invalid author ID"))?;
        let myself = self.myself().await?;
        if Some(owner) != myself["id"].as_number() {
            return Err(Error::new(
                "input",
                "Only your own comment body can be updated",
            ));
        }
        let current = latest["content"]
            .as_str()
            .ok_or_else(|| Error::new("input", "Change history has no editable comment body"))?;
        if current == body {
            return Err(Error::new("input", "Comment body has not changed"));
        }
        let value = self.api.update_comment(issue, comment_id, body).await?;
        if !valid_comment(&value) {
            return Err(
                invalid_response("Updated comment response has invalid required fields")
                    .with_write_outcome("comment.update", WriteOutcome::Updated),
            );
        }
        Ok(value)
    }
    pub async fn comment_add(&self, issue: &str, add: CommentAdd<'_>) -> Result<Value, Error> {
        self.add_comment_inner(issue, add)
            .await
            .map_err(|error| error.with_write_outcome("comment.add", WriteOutcome::NotCreated))
    }
    async fn add_comment_inner(&self, issue: &str, add: CommentAdd<'_>) -> Result<Value, Error> {
        validate_issue_identifier(issue)?;
        let mut form = if add.status.is_some()
            || add.assignee.is_some()
            || add.resolution.is_some()
            || add.clear_assignee
            || add.clear_resolution
        {
            self.issue_update_form(
                issue,
                IssueUpdate {
                    status: add.status,
                    assignee: add.assignee,
                    resolution: add.resolution,
                    clear_assignee: add.clear_assignee,
                    clear_resolution: add.clear_resolution,
                    ..Default::default()
                },
            )
            .await?
        } else {
            Vec::new()
        };
        let body = add.body.filter(|body| !body.is_empty());
        if form.is_empty() {
            let body = body.ok_or_else(|| Error::new("input", "No body or fields have changed"))?;
            let value = self.api.add_comment(issue, body).await?;
            if !value
                .get("id")
                .and_then(Value::as_number)
                .is_some_and(|id| positive_integer(&id.to_string()))
            {
                let mut error = Error::new("invalid_response", "Comment response has invalid ID");
                error.http_status = Some(201);
                return Err(error.with_write_outcome("comment.add", WriteOutcome::Created));
            }
            Ok(serde_json::json!({"resource":"comment","data":value}))
        } else {
            if let Some(body) = body {
                form.push(("comment", body.into()));
            }
            let value = self.api.update_issue_for_comment(issue, &form).await?;
            if !valid_written_issue(&value) {
                return Err(
                    invalid_response("Updated issue response has invalid required fields")
                        .with_write_outcome("comment.add", WriteOutcome::Updated),
                );
            }
            Ok(serde_json::json!({"resource":"issue","data":value}))
        }
    }
    pub async fn issue_update(&self, issue: &str, update: IssueUpdate<'_>) -> Result<Value, Error> {
        self.update_issue_inner(issue, update)
            .await
            .map_err(|error| error.with_update_outcome(WriteOutcome::NotUpdated))
    }
    async fn update_issue_inner(
        &self,
        issue: &str,
        update: IssueUpdate<'_>,
    ) -> Result<Value, Error> {
        let form = self.issue_update_form(issue, update).await?;
        if form.is_empty() {
            return Err(Error::new("input", "No fields have changed"));
        }
        let mut value = self.api.update_issue(issue, &form).await?;
        if !valid_written_issue(&value) {
            return Err(
                invalid_response("Updated issue response has invalid required fields")
                    .with_update_outcome(WriteOutcome::Updated),
            );
        }
        let key = value["issueKey"].as_str().expect("validated key");
        let url = self.api.issue_url(key);
        value["url"] = Value::String(url);
        Ok(value)
    }
    async fn issue_update_form(
        &self,
        issue: &str,
        update: IssueUpdate<'_>,
    ) -> Result<Vec<(&'static str, String)>, Error> {
        validate_issue_identifier(issue)?;
        if update.title.is_some_and(|title| title.trim().is_empty()) {
            return Err(Error::new("input", "Title must not be blank"));
        }
        for (clear, specified) in [
            (update.clear_assignee, update.assignee.is_some()),
            (update.clear_resolution, update.resolution.is_some()),
            (update.clear_start_date, update.start_date.is_some()),
            (update.clear_due_date, update.due_date.is_some()),
            (update.clear_milestones, !update.milestone.is_empty()),
        ] {
            if clear && specified {
                return Err(Error::new(
                    "input",
                    "Value and clear flag are mutually exclusive",
                ));
            }
        }
        for date in [update.start_date, update.due_date].into_iter().flatten() {
            if !valid_date(date) {
                return Err(Error::new("input", "Date must be a real YYYY-MM-DD date"));
            }
        }
        if update
            .milestone
            .iter()
            .any(|s| matches!(s, Selection::Name(_)))
            && update
                .milestone
                .iter()
                .any(|s| matches!(s, Selection::Id(_)))
        {
            return Err(Error::new(
                "input",
                "Milestone names and IDs are mutually exclusive",
            ));
        }
        // Direct IDs need no candidates, and invalid IDs fail before fetching the issue.
        for (candidate, selection) in [
            (Candidate::IssueType { project: "" }, update.issue_type),
            (Candidate::Priority, update.priority),
            (Candidate::Status { project: "" }, update.status),
            (Candidate::Assignee { project: "" }, update.assignee),
            (Candidate::Resolution, update.resolution),
        ] {
            if let Some(Selection::Id(id)) = selection {
                self.resolve(candidate, Selection::Id(id)).await?;
            }
        }
        for selection in &update.milestone {
            if let Selection::Id(id) = selection {
                self.resolve(Candidate::Milestone { project: "" }, Selection::Id(id))
                    .await?;
            }
        }
        let latest = self.api.get(&["issues", issue]).await?;
        let project = response_id(&latest["projectId"], false)?.to_string();
        let project = project.as_str();
        let mut form = Vec::new();
        if let Some(title) = update.title {
            let current = latest
                .get("summary")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid_response("Issue response has invalid summary"))?;
            if title != current {
                form.push(("summary", title.into()));
            }
        }
        if let Some(body) = update.body {
            let current = latest
                .get("description")
                .ok_or_else(|| invalid_response("Issue response has invalid description"))?;
            let current = if current.is_null() {
                ""
            } else {
                current
                    .as_str()
                    .ok_or_else(|| invalid_response("Issue response has invalid description"))?
            };
            if body != current {
                form.push(("description", body.into()));
            }
        }
        for (field, parameter, candidate, selection, clear) in [
            (
                "issueType",
                "issueTypeId",
                Candidate::IssueType { project },
                update.issue_type,
                false,
            ),
            (
                "priority",
                "priorityId",
                Candidate::Priority,
                update.priority,
                false,
            ),
            (
                "status",
                "statusId",
                Candidate::Status { project },
                update.status,
                false,
            ),
            (
                "assignee",
                "assigneeId",
                Candidate::Assignee { project },
                update.assignee,
                update.clear_assignee,
            ),
            (
                "resolution",
                "resolutionId",
                Candidate::Resolution,
                update.resolution,
                update.clear_resolution,
            ),
        ] {
            if selection.is_none() && !clear {
                continue;
            }
            let current = latest
                .get(field)
                .ok_or_else(|| invalid_response("Issue response has invalid comparison field"))?;
            let nullable = matches!(
                candidate,
                Candidate::Assignee { .. } | Candidate::Resolution
            );
            let current = if nullable && current.is_null() {
                None
            } else {
                Some(response_id(
                    &current["id"],
                    matches!(candidate, Candidate::Resolution),
                )?)
            };
            let desired = match selection {
                Some(selection) => Some(self.resolve(candidate, selection).await?),
                None => None,
            };
            if current != desired {
                form.push((
                    parameter,
                    desired.map(|id| id.to_string()).unwrap_or_default(),
                ));
            }
        }
        for (field, date, clear) in [
            ("startDate", update.start_date, update.clear_start_date),
            ("dueDate", update.due_date, update.clear_due_date),
        ] {
            if date.is_none() && !clear {
                continue;
            }
            let current = latest
                .get(field)
                .ok_or_else(|| invalid_response("Issue response has invalid date"))?;
            let current = if current.is_null() {
                None
            } else {
                let text = current
                    .as_str()
                    .ok_or_else(|| invalid_response("Issue response has invalid date"))?;
                // Backlog date fields include a time suffix; compare only the calendar date.
                Some(
                    text.get(..10)
                        .filter(|date| valid_date(date))
                        .ok_or_else(|| invalid_response("Issue response has invalid date"))?,
                )
            };
            if current != date {
                form.push((field, date.unwrap_or_default().into()));
            }
        }
        if !update.milestone.is_empty() || update.clear_milestones {
            let mut current = latest
                .get("milestone")
                .and_then(Value::as_array)
                .ok_or_else(|| invalid_response("Issue response has invalid milestones"))?
                .iter()
                .map(|v| response_id(&v["id"], false).map(|id| id.to_string()))
                .collect::<Result<Vec<_>, _>>()?;
            let desired = self
                .resolve_search_values(Candidate::Milestone { project }, update.milestone)
                .await?;
            current.sort();
            current.dedup();
            let mut sorted = desired.clone();
            sorted.sort();
            if current != sorted {
                if desired.is_empty() {
                    form.push(("milestoneId[]", String::new()));
                } else {
                    form.extend(desired.into_iter().map(|id| ("milestoneId[]", id)));
                }
            }
        }
        Ok(form)
    }
    pub async fn issue_create(
        &self,
        settings: &Settings,
        create: IssueCreate<'_>,
    ) -> Result<Value, Error> {
        self.create_issue_inner(settings, create)
            .await
            .map_err(|error| error.with_create_outcome(CreateOutcome::NotCreated))
    }
    async fn create_issue_inner(
        &self,
        settings: &Settings,
        create: IssueCreate<'_>,
    ) -> Result<Value, Error> {
        if create.title.trim().is_empty() {
            return Err(Error::new("input", "Title must not be blank"));
        }
        if create.clear_assignee && create.assignee.is_some() {
            return Err(Error::new(
                "input",
                "Assignee and clear-assignee are mutually exclusive",
            ));
        }
        for date in [create.start_date, create.due_date].into_iter().flatten() {
            if !valid_date(date) {
                return Err(Error::new("input", "Date must be a real YYYY-MM-DD date"));
            }
        }
        if create
            .milestone
            .iter()
            .any(|s| matches!(s, Selection::Name(_)))
            && create
                .milestone
                .iter()
                .any(|s| matches!(s, Selection::Id(_)))
        {
            return Err(Error::new(
                "input",
                "Milestone names and IDs are mutually exclusive",
            ));
        }
        let project = create
            .project
            .or(settings.project.as_deref())
            .ok_or_else(|| Error::new("input", "Project is required"))?;
        let issue_type = create
            .issue_type
            .or(settings
                .issue
                .create
                .issue_type
                .as_deref()
                .map(Selection::Name))
            .ok_or_else(|| Error::new("input", "Issue type is required"))?;
        let priority = create
            .priority
            .or(settings
                .issue
                .create
                .priority
                .as_deref()
                .map(Selection::Name))
            .ok_or_else(|| Error::new("input", "Priority is required"))?;
        if matches!(project, "" | "." | ".." | "all") {
            return Err(Error::new("input", "A concrete project is required"));
        }
        // Validate all explicit IDs before any name-resolution request.
        for (candidate, selection) in [
            (Candidate::IssueType { project }, Some(issue_type)),
            (Candidate::Priority, Some(priority)),
            (Candidate::Assignee { project }, create.assignee),
        ] {
            if let Some(Selection::Id(id)) = selection {
                self.resolve(candidate, Selection::Id(id)).await?;
            }
        }
        for selection in &create.milestone {
            if let Selection::Id(id) = selection {
                self.resolve(Candidate::Milestone { project }, Selection::Id(id))
                    .await?;
            }
        }
        let project_id = if project.bytes().all(|c| c.is_ascii_digit()) {
            self.resolve(Candidate::IssueType { project }, Selection::Id(project))
                .await?
                .to_string()
        } else {
            let value = self.api.get(&["projects", project]).await?;
            value
                .get("id")
                .and_then(Value::as_number)
                .filter(|id| positive_integer(&id.to_string()))
                .ok_or_else(|| invalid_response("Project response has invalid ID"))?
                .to_string()
        };
        let mut form = vec![
            ("projectId", project_id),
            ("summary", create.title.into()),
            (
                "issueTypeId",
                self.resolve(Candidate::IssueType { project }, issue_type)
                    .await?
                    .to_string(),
            ),
            (
                "priorityId",
                self.resolve(Candidate::Priority, priority)
                    .await?
                    .to_string(),
            ),
        ];
        let assignee = if create.clear_assignee {
            None
        } else {
            create.assignee.or(settings
                .issue
                .create
                .assignee
                .as_deref()
                .map(Selection::Name))
        };
        if let Some(assignee) = assignee {
            form.push((
                "assigneeId",
                self.resolve(Candidate::Assignee { project }, assignee)
                    .await?
                    .to_string(),
            ));
        }
        for (field, date) in [
            ("startDate", create.start_date),
            ("dueDate", create.due_date),
        ] {
            if let Some(date) = date {
                form.push((field, date.into()));
            }
        }
        if let Some(body) = create.body {
            form.push(("description", body.into()));
        }
        for id in self
            .resolve_search_values(Candidate::Milestone { project }, create.milestone)
            .await?
        {
            form.push(("milestoneId[]", id));
        }
        let mut value = self.api.create_issue(&form).await?;
        let valid_id = value
            .get("id")
            .and_then(Value::as_number)
            .is_some_and(|id| positive_integer(&id.to_string()));
        let key = value
            .get("issueKey")
            .and_then(Value::as_str)
            .filter(|key| issue_key(key) && valid_id)
            .ok_or_else(|| {
                let mut error = Error::new(
                    "invalid_response",
                    "Created issue response has invalid required fields",
                )
                .with_create_outcome(CreateOutcome::Created);
                error.http_status = Some(201);
                error
            })?;
        let url = self.api.issue_url(key);
        value["url"] = Value::String(url);
        Ok(value)
    }
    /// Fetch one descending comment page; ID boundaries are exclusive at the API.
    pub async fn comment_list(
        &self,
        issue_id_or_key: &str,
        limit: u8,
        min_id: Option<&str>,
        max_id: Option<&str>,
    ) -> Result<Value, Error> {
        validate_issue_identifier(issue_id_or_key)?;
        if !(1..=100).contains(&limit) {
            return Err(Error::new("input", "Limit must be between 1 and 100"));
        }
        for id in [min_id, max_id].into_iter().flatten() {
            if !positive_integer(id) {
                return Err(Error::new(
                    "input",
                    "Comment boundary must be a positive integer",
                ));
            }
        }
        if let (Some(min), Some(max)) = (min_id, max_id) {
            let min = min.trim_start_matches('0');
            let max = max.trim_start_matches('0');
            if (min.len(), min) >= (max.len(), max) {
                return Err(Error::new(
                    "input",
                    "Comment min ID must be less than max ID",
                ));
            }
        }
        let mut query = vec![("count", limit.to_string()), ("order", "desc".into())];
        if let Some(id) = min_id {
            query.push(("minId", id.into()));
        }
        if let Some(id) = max_id {
            query.push(("maxId", id.into()));
        }
        let items = self
            .api
            .get_query(&["issues", issue_id_or_key, "comments"], &query)
            .await?;
        if !items
            .as_array()
            .is_some_and(|items| items.iter().all(valid_comment))
        {
            return Err(invalid_response(
                "Comments response has invalid required fields",
            ));
        }
        Ok(serde_json::json!({"items":items}))
    }
    pub async fn comment_view(
        &self,
        issue_id_or_key: &str,
        comment_id: &str,
    ) -> Result<Value, Error> {
        validate_issue_identifier(issue_id_or_key)?;
        if !positive_integer(comment_id) {
            return Err(Error::new("input", "Comment ID must be a positive integer"));
        }
        let value = self
            .api
            .get(&["issues", issue_id_or_key, "comments", comment_id])
            .await?;
        if !valid_comment(&value) {
            return Err(invalid_response(
                "Comment response has invalid required fields",
            ));
        }
        Ok(value)
    }
    /// Preserve the API object and add a URL from the configured origin and response key.
    pub async fn issue_view(&self, issue_id_or_key: &str) -> Result<Value, Error> {
        if !positive_integer(issue_id_or_key) && !issue_key(issue_id_or_key) {
            return Err(Error::new(
                "input",
                "Issue must be a positive ID or issue key",
            ));
        }
        let mut value = self.api.get(&["issues", issue_id_or_key]).await?;
        let named = |v: &Value| v.get("name").is_some_and(Value::is_string);
        let valid = ["id", "projectId"].iter().all(|field| {
            value
                .get(field)
                .and_then(Value::as_number)
                .is_some_and(|id| positive_integer(&id.to_string()))
        }) && value
            .get("issueKey")
            .and_then(Value::as_str)
            .is_some_and(issue_key)
            && ["summary", "created", "updated"]
                .iter()
                .all(|field| value.get(field).is_some_and(Value::is_string))
            && ["description", "startDate", "dueDate"].iter().all(|field| {
                value
                    .get(field)
                    .is_some_and(|v| v.is_null() || v.is_string())
            })
            && ["issueType", "priority", "status"]
                .iter()
                .all(|field| value.get(field).is_some_and(named))
            && ["assignee", "resolution"]
                .iter()
                .all(|field| value.get(field).is_some_and(|v| v.is_null() || named(v)))
            && value
                .get("milestone")
                .and_then(Value::as_array)
                .is_some_and(|items| items.iter().all(named));
        if !valid {
            return Err(invalid_response(
                "Issue response has invalid required fields",
            ));
        }
        let url = self
            .api
            .issue_url(value["issueKey"].as_str().expect("validated issue key"));
        value["url"] = Value::String(url);
        Ok(value)
    }
    /// Comments have no project ID; resolve the target issue only for fixed body display.
    pub async fn issue_text_formatting_rule(&self, issue_id_or_key: &str) -> Result<String, Error> {
        validate_issue_identifier(issue_id_or_key)?;
        let issue = self.api.get(&["issues", issue_id_or_key]).await?;
        let project_id = issue
            .get("projectId")
            .and_then(Value::as_number)
            .filter(|id| positive_integer(&id.to_string()))
            .ok_or_else(|| invalid_response("Issue response has invalid projectId"))?;
        self.project_text_formatting_rule(project_id).await
    }
    /// Look up the target issue's project rule only when preparing body display.
    pub async fn project_text_formatting_rule(&self, project_id: &Number) -> Result<String, Error> {
        if !positive_integer(&project_id.to_string()) {
            return Err(Error::new("input", "Project ID must be a positive integer"));
        }
        let project = self.api.get(&["projects", &project_id.to_string()]).await?;
        project
            .get("textFormattingRule")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| invalid_response("Project response has invalid textFormattingRule"))
    }
    /// Return exactly one page, preserving the original items and arbitrary-precision total.
    pub async fn search(
        &self,
        settings: &Settings,
        search: IssueSearch<'_>,
    ) -> Result<Value, Error> {
        let resolved = self.resolve_search(settings, search).await?;
        self.search_resolved(&resolved).await
    }
    pub async fn resolve_search(
        &self,
        settings: &Settings,
        search: IssueSearch<'_>,
    ) -> Result<ResolvedIssueSearch, Error> {
        if !(1..=100).contains(&search.limit) {
            return Err(Error::new("input", "Limit must be between 1 and 100"));
        }
        let offset = search_offset(search.offset)?;
        let project = search
            .project
            .or(settings.project.as_deref())
            .ok_or_else(|| {
                Error::new(
                    "input",
                    "Project is required; use --project or cwd settings",
                )
            })?;
        if search.mine && !search.assignee.is_empty() {
            return Err(Error::new("input", "Mine does not accept assignee inputs"));
        }
        let assignee = if search.mine {
            vec![Selection::Name("self")]
        } else {
            search.assignee
        };
        let cross_project = project == "all";
        if cross_project
            && (!search.status.is_empty()
                || !search.milestone.is_empty()
                || assignee.iter().any(|s| *s != Selection::Name("self"))
                || (!assignee.contains(&Selection::Name("self"))
                    && search.keyword.is_none_or(|k| k.trim().is_empty())))
        {
            return Err(Error::new(
                "input",
                "Cross-project search requires keyword or self, and rejects status, milestone and other assignees",
            ));
        }
        let clear_status = search.status.contains(&Selection::Name("all"));
        if clear_status && search.status.iter().any(|s| *s != Selection::Name("all")) {
            return Err(Error::new(
                "input",
                "Status all cannot be combined with other statuses",
            ));
        }
        for (candidate, selections) in [
            (Candidate::Status { project }, &search.status),
            (Candidate::Assignee { project }, &assignee),
            (Candidate::Milestone { project }, &search.milestone),
        ] {
            if selections.iter().any(|s| matches!(s, Selection::Name(_)))
                && selections.iter().any(|s| matches!(s, Selection::Id(_)))
            {
                return Err(Error::new(
                    "input",
                    "Names and IDs for the same field are mutually exclusive",
                ));
            }
            for selection in selections {
                if matches!(selection, Selection::Id(_)) {
                    self.resolve(candidate, *selection).await?;
                }
            }
        }
        let mut query = Vec::new();
        if !cross_project {
            let id = if !project.is_empty() && project.bytes().all(|c| c.is_ascii_digit()) {
                self.resolve(Candidate::Status { project }, Selection::Id(project))
                    .await?
            } else {
                let value = self.api.get(&["projects", project]).await?;
                value
                    .get("id")
                    .and_then(Value::as_number)
                    .filter(|id| {
                        let s = id.to_string();
                        s != "0" && s.bytes().all(|c| c.is_ascii_digit())
                    })
                    .cloned()
                    .ok_or_else(|| {
                        invalid_response("Project response has invalid required fields")
                    })?
            };
            query.push(("projectId[]", id.to_string()));
        }
        if let Some(keyword) = search.keyword {
            query.push(("keyword", keyword.into()));
        }
        let status = if clear_status || cross_project {
            vec![]
        } else if search.status.is_empty() {
            settings
                .issue
                .list
                .status
                .iter()
                .map(|name| Selection::Name(name))
                .collect()
        } else {
            search.status
        };
        for (field, candidate, selections) in [
            ("statusId[]", Candidate::Status { project }, status),
            ("assigneeId[]", Candidate::Assignee { project }, assignee),
            (
                "milestoneId[]",
                Candidate::Milestone { project },
                search.milestone,
            ),
        ] {
            for id in self.resolve_search_values(candidate, selections).await? {
                query.push((field, id));
            }
        }
        query.extend([("sort", "updated".into()), ("order", "desc".into())]);
        Ok(ResolvedIssueSearch {
            query,
            limit: search.limit,
            offset,
        })
    }
    /// Count and list must both succeed before the caller exchanges the visible search.
    pub async fn search_resolved(&self, search: &ResolvedIssueSearch) -> Result<Value, Error> {
        let count = self
            .api
            .get_query(&["issues", "count"], &search.query)
            .await?;

        let total = count
            .get("count")
            .filter(|n| {
                n.as_number()
                    .is_some_and(|n| n.to_string().bytes().all(|c| c.is_ascii_digit()))
            })
            .ok_or_else(|| invalid_response("Count response has invalid required fields"))?;
        let items = self.search_page(search, &search.offset.to_string()).await?;
        Ok(
            serde_json::json!({"items": items, "pagination": {"offset": search.offset, "limit": search.limit, "count": items.as_array().unwrap().len(), "total": total}}),
        )
    }
    /// Fetch a single page without recounting or resolving names again; preserve API JSON.
    pub async fn search_page(
        &self,
        search: &ResolvedIssueSearch,
        offset: &str,
    ) -> Result<Value, Error> {
        let offset = search_offset(offset)?;
        let mut query = search.query.clone();
        query.extend([
            ("count", search.limit.to_string()),
            ("offset", offset.to_string()),
        ]);
        let items = self.api.get_query(&["issues"], &query).await?;
        items
            .as_array()
            .filter(|items| {
                items.iter().all(|item| {
                    item.get("id").and_then(Value::as_number).is_some_and(|id| {
                        let s = id.to_string();
                        s != "0" && s.bytes().all(|c| c.is_ascii_digit())
                    }) && item.get("issueKey").is_some_and(Value::is_string)
                        && item.get("summary").is_some_and(Value::is_string)
                        && item
                            .get("status")
                            .and_then(|s| s.get("name"))
                            .is_some_and(Value::is_string)
                })
            })
            .ok_or_else(|| invalid_response("Issues response has invalid required fields"))?;
        Ok(items)
    }
    async fn resolve_search_values(
        &self,
        candidate: Candidate<'_>,
        selections: Vec<Selection<'_>>,
    ) -> Result<Vec<String>, Error> {
        let mut seen = Vec::new();
        let mut ids = Vec::new();
        let mut items = None;
        for selection in selections {
            if seen.contains(&selection) {
                continue;
            }
            seen.push(selection);
            let id = match selection {
                Selection::Name(name)
                    if !(matches!(candidate, Candidate::Assignee { .. }) && name == "self") =>
                {
                    if items.is_none() {
                        items = Some(self.candidates(candidate).await?);
                    }
                    resolve_name(items.as_ref().expect("loaded candidates"), name)?
                }
                _ => self.resolve(candidate, selection).await?,
            }
            .to_string();
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        Ok(ids)
    }
    /// Fetch only this endpoint; retain the API JSON without caching or display conversion.
    pub async fn candidates(&self, candidate: Candidate<'_>) -> Result<Value, Error> {
        let value = match candidate {
            Candidate::Priority => self.api.get(&["priorities"]).await?,
            Candidate::Resolution => self.api.get(&["resolutions"]).await?,
            Candidate::IssueType { project } => {
                self.api.get(&["projects", project, "issueTypes"]).await?
            }
            Candidate::Status { project } => {
                self.api.get(&["projects", project, "statuses"]).await?
            }
            Candidate::Assignee { project } => {
                self.api.get(&["projects", project, "users"]).await?
            }
            Candidate::Milestone { project } => {
                self.api.get(&["projects", project, "versions"]).await?
            }
        };
        if !value.as_array().is_some_and(|items| {
            items.iter().all(|item| {
                item.get("id").and_then(Value::as_number).is_some_and(|n| {
                    let s = n.to_string();
                    (s != "0" || matches!(candidate, Candidate::Resolution))
                        && s.bytes().all(|c| c.is_ascii_digit())
                }) && item.get("name").is_some_and(Value::is_string)
            })
        }) {
            let mut error = Error::new(
                "invalid_response",
                "Candidates response has invalid required fields",
            );
            error.http_status = Some(200);
            return Err(error);
        }
        Ok(value)
    }
    pub async fn myself(&self) -> Result<Value, Error> {
        let value = self.api.get(&["users", "myself"]).await?;
        if !value
            .get("id")
            .and_then(Value::as_number)
            .is_some_and(|id| {
                let s = id.to_string();
                s != "0" && s.bytes().all(|c| c.is_ascii_digit())
            })
        {
            let mut error = Error::new(
                "invalid_response",
                "Myself response has invalid required fields",
            );
            error.http_status = Some(200);
            return Err(error);
        }
        Ok(value)
    }
    /// Resolve to an arbitrary-precision ID; resolution 0 is a value, not a clear operation.
    pub async fn resolve(
        &self,
        candidate: Candidate<'_>,
        selection: Selection<'_>,
    ) -> Result<Number, Error> {
        let name = match selection {
            Selection::Id(id) => {
                if id.is_empty()
                    || !id.bytes().all(|c| c.is_ascii_digit())
                    || (!matches!(candidate, Candidate::Resolution)
                        && id.bytes().all(|c| c == b'0'))
                {
                    return Err(Error::new(
                        "input",
                        "ID must be a positive integer, or 0 for resolution",
                    ));
                }
                let canonical = id.trim_start_matches('0');
                return if canonical.is_empty() { "0" } else { canonical }
                    .parse()
                    .map_err(|_| Error::new("input", "Invalid ID"));
            }
            Selection::Name(name) => name,
        };
        if matches!(candidate, Candidate::Assignee { .. }) && name == "self" {
            return Ok(self.myself().await?["id"]
                .as_number()
                .expect("validated ID")
                .clone());
        }
        let items = self.candidates(candidate).await?;
        resolve_name(&items, name)
    }
}

fn valid_written_issue(value: &Value) -> bool {
    value
        .get("issueKey")
        .and_then(Value::as_str)
        .is_some_and(issue_key)
        && value
            .get("id")
            .and_then(Value::as_number)
            .is_some_and(|id| positive_integer(&id.to_string()))
}

fn response_id(value: &Value, zero: bool) -> Result<Number, Error> {
    value
        .as_number()
        .filter(|id| {
            let text = id.to_string();
            positive_integer(&text) || (zero && text == "0")
        })
        .cloned()
        .ok_or_else(|| invalid_response("Issue response has invalid ID"))
}

fn resolve_name(items: &Value, name: &str) -> Result<Number, Error> {
    let mut matches = items
        .as_array()
        .expect("validated candidates array")
        .iter()
        .filter(|item| item["name"].as_str() == Some(name));
    let matched = matches
        .next()
        .ok_or_else(|| Error::new("input", "No candidate matches the exact name"))?;
    if matches.next().is_some() {
        return Err(Error::new(
            "input",
            "Multiple candidates match the exact name",
        ));
    }
    Ok(matched["id"].as_number().expect("validated ID").clone())
}

fn invalid_response(message: &str) -> Error {
    let mut error = Error::new("invalid_response", message);
    error.http_status = Some(200);
    error
}

fn validate_issue_identifier(value: &str) -> Result<(), Error> {
    if !positive_integer(value) && !issue_key(value) {
        return Err(Error::new(
            "input",
            "Issue must be a positive ID or issue key",
        ));
    }
    Ok(())
}

fn valid_comment(value: &Value) -> bool {
    value
        .get("id")
        .and_then(Value::as_number)
        .is_some_and(|id| positive_integer(&id.to_string()))
        && value
            .get("content")
            .is_some_and(|v| v.is_null() || v.is_string())
        && value
            .get("createdUser")
            .and_then(|v| v.get("name"))
            .is_some_and(Value::is_string)
        && ["created", "updated"]
            .iter()
            .all(|field| value.get(field).is_some_and(Value::is_string))
        && value.get("changeLog").is_some_and(|v| {
            v.is_null()
                || v.as_array().is_some_and(|items| {
                    items.iter().all(|item| {
                        item.get("field").is_some_and(Value::is_string)
                            && item.get("originalValue").is_some()
                            && item.get("newValue").is_some()
                    })
                })
        })
}

fn positive_integer(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|c| c.is_ascii_digit())
        && value.bytes().any(|c| c != b'0')
}

pub(crate) fn issue_key(value: &str) -> bool {
    value.rsplit_once('-').is_some_and(|(project, number)| {
        !project.is_empty()
            && project
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_')
            && positive_integer(number)
    })
}

fn valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
    {
        return false;
    }
    let year: u16 = value[..4].parse().expect("four digits");
    let month: u8 = value[5..7].parse().expect("two digits");
    let day: u8 = value[8..].parse().expect("two digits");
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year.is_multiple_of(400) || (year.is_multiple_of(4) && !year.is_multiple_of(100)) => {
            29
        }
        2 => 28,
        _ => 0,
    };
    year != 0 && day != 0 && day <= days
}

fn search_offset(offset: &str) -> Result<Number, Error> {
    if offset.is_empty() || !offset.bytes().all(|c| c.is_ascii_digit()) {
        return Err(Error::new("input", "Offset must be a nonnegative integer"));
    }
    match offset.trim_start_matches('0') {
        "" => "0",
        n => n,
    }
    .parse()
    .map_err(|_| Error::new("input", "Invalid offset"))
}
