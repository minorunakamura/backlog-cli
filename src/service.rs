use crate::{Error, api::Api, config::Config};
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
pub enum Selection<'a> {
    Name(&'a str),
    Id(&'a str),
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
}
