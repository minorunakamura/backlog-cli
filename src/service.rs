use crate::{
    Error,
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

pub struct Service {
    api: Api,
}
impl Service {
    pub fn new(config: &Config) -> Result<Self, Error> {
        Ok(Self {
            api: Api::new(config)?,
        })
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
        if !(1..=100).contains(&search.limit) {
            return Err(Error::new("input", "Limit must be between 1 and 100"));
        }
        if search.offset.is_empty() || !search.offset.bytes().all(|c| c.is_ascii_digit()) {
            return Err(Error::new("input", "Offset must be a nonnegative integer"));
        }
        let offset: Number = match search.offset.trim_start_matches('0') {
            "" => "0",
            n => n,
        }
        .parse()
        .map_err(|_| Error::new("input", "Invalid offset"))?;
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
        let count = self.api.get_query(&["issues", "count"], &query).await?;
        let total = count
            .get("count")
            .filter(|n| {
                n.as_number()
                    .is_some_and(|n| n.to_string().bytes().all(|c| c.is_ascii_digit()))
            })
            .ok_or_else(|| invalid_response("Count response has invalid required fields"))?;
        query.extend([
            ("count", search.limit.to_string()),
            ("offset", offset.to_string()),
        ]);
        let items = self.api.get_query(&["issues"], &query).await?;
        let length = items
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
            .ok_or_else(|| invalid_response("Issues response has invalid required fields"))?
            .len();
        Ok(
            serde_json::json!({"items": items, "pagination": {"offset": offset, "limit": search.limit, "count": length, "total": total}}),
        )
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
