use crate::{Error, WriteOutcome, config::Config};
use reqwest::{Client, header::HeaderValue, redirect::Policy};
use serde::{Deserializer, de::MapAccess, de::Visitor};
use serde_json::{Value, value::RawValue};
use std::time::Duration;

fn decode_json(raw: &RawValue, remaining_depth: u8) -> Result<Value, serde_json::Error> {
    let text = raw.get();
    // Inspect JSON syntax, not serde_json's synthetic arbitrary-precision number map.
    match text.as_bytes()[0] {
        b'{' | b'[' if remaining_depth == 1 => {
            Err(serde::de::Error::custom("recursion limit exceeded"))
        }
        b'{' => {
            struct ObjectVisitor(u8);
            impl<'de> Visitor<'de> for ObjectVisitor {
                type Value = Value;

                fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                    formatter.write_str("a JSON object")
                }

                fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Value, M::Error> {
                    let mut object = serde_json::Map::new();
                    while let Some((key, raw)) = map.next_entry::<String, &RawValue>()? {
                        // Validate each value before a later duplicate can replace it.
                        let value =
                            decode_json(raw, self.0 - 1).map_err(serde::de::Error::custom)?;
                        object.insert(key, value);
                    }
                    Ok(Value::Object(object))
                }
            }
            serde_json::Deserializer::from_str(text).deserialize_map(ObjectVisitor(remaining_depth))
        }
        b'[' => Ok(Value::Array(
            serde_json::from_str::<Vec<&RawValue>>(text)?
                .into_iter()
                .map(|value| decode_json(value, remaining_depth - 1))
                .collect::<Result<_, _>>()?,
        )),
        _ => serde_json::from_str(text),
    }
}

pub struct Api {
    client: Client,
    origin: reqwest::Url,
    key: HeaderValue,
}
impl Api {
    pub fn new(config: &Config) -> Result<Self, Error> {
        let mut key = HeaderValue::from_str(&config.api_key)
            .map_err(|_| Error::new("config", "API key cannot be sent as a header"))?;
        key.set_sensitive(true);
        let client = Client::builder()
            .redirect(Policy::none())
            .retry(reqwest::retry::never())
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|_| Error::new("network", "Cannot initialize HTTPS client"))?;
        Ok(Self {
            client,
            origin: config.origin.clone(),
            key,
        })
    }
    pub async fn project_list(&self) -> Result<Value, Error> {
        let value = self.get(&["projects"]).await?;
        if !value.as_array().is_some_and(|items| {
            items.iter().all(|item| {
                item.is_object()
                    && item.get("id").is_some_and(|id| {
                        id.as_number().is_some_and(|n| {
                            let s = n.to_string();
                            !s.starts_with('0') && s.bytes().all(|c| c.is_ascii_digit())
                        })
                    })
                    && item.get("projectKey").is_some_and(Value::is_string)
                    && item.get("name").is_some_and(Value::is_string)
            })
        }) {
            let mut error = Error::new(
                "invalid_response",
                "Projects response has invalid required fields",
            );
            error.http_status = Some(200);
            return Err(error);
        }
        Ok(value)
    }
    pub(crate) async fn create_issue(&self, form: &[(&str, String)]) -> Result<Value, Error> {
        self.write(&["issues"], reqwest::Method::POST, "issue.create", form)
            .await
    }
    pub(crate) async fn update_issue(
        &self,
        issue: &str,
        form: &[(&str, String)],
    ) -> Result<Value, Error> {
        self.write(
            &["issues", issue],
            reqwest::Method::PATCH,
            "issue.update",
            form,
        )
        .await
    }
    pub(crate) async fn add_comment(&self, issue: &str, body: &str) -> Result<Value, Error> {
        self.write(
            &["issues", issue, "comments"],
            reqwest::Method::POST,
            "comment.add",
            &[("content", body.into())],
        )
        .await
    }
    pub(crate) async fn update_comment(
        &self,
        issue: &str,
        comment: &str,
        body: &str,
    ) -> Result<Value, Error> {
        self.write(
            &["issues", issue, "comments", comment],
            reqwest::Method::PATCH,
            "comment.update",
            &[("content", body.into())],
        )
        .await
    }
    pub(crate) async fn update_issue_for_comment(
        &self,
        issue: &str,
        form: &[(&str, String)],
    ) -> Result<Value, Error> {
        self.write(
            &["issues", issue],
            reqwest::Method::PATCH,
            "comment.add",
            form,
        )
        .await
    }
    async fn write(
        &self,
        segments: &[&str],
        method: reqwest::Method,
        operation: &'static str,
        form: &[(&str, String)],
    ) -> Result<Value, Error> {
        let (not_written, written, expected_status) = if method == reqwest::Method::POST {
            (WriteOutcome::NotCreated, WriteOutcome::Created, 201)
        } else {
            (WriteOutcome::NotUpdated, WriteOutcome::Updated, 200)
        };
        let mut url = self.origin.clone();
        url.path_segments_mut()
            .expect("validated origin")
            .clear()
            .extend(["api", "v2"])
            .extend(segments);
        let request = self
            .client
            .request(method, url)
            .header("Backlog-API-Key", self.key.clone())
            .form(form)
            .build()
            .map_err(|_| {
                Error::new("input", "Cannot construct write request")
                    .with_write_outcome(operation, not_written)
            })?;
        let state = std::sync::Arc::new(std::sync::Mutex::new((not_written, None)));
        let progress = state.clone();
        let client = self.client.clone();
        let mut task = tokio::spawn(async move {
            // Record possible transmission before send, and success headers before reading body.
            progress.lock().expect("write state").0 = WriteOutcome::Unknown;
            let response = client.execute(request).await.map_err(|error| {
                if error.is_connect() {
                    // Connection establishment failed before the HTTP request could be sent.
                    progress.lock().expect("write state").0 = not_written;
                }
                Error::new("network", "HTTPS request failed or timed out")
            })?;
            let status = response.status().as_u16();
            *progress.lock().expect("write state") = (
                if status == expected_status {
                    written
                } else {
                    WriteOutcome::Unknown
                },
                Some(status),
            );
            let body = response
                .bytes()
                .await
                .map_err(|_| Error::new("network", "Response body failed or timed out"))?;
            let decoded =
                serde_json::from_slice::<&RawValue>(&body).and_then(|raw| decode_json(raw, 128));
            if status != expected_status {
                let kind = match status {
                    401 => "auth",
                    403 => "permission",
                    404 => "not_found",
                    429 => "rate_limit",
                    _ => "api",
                };
                let mut error = Error::new(
                    kind,
                    "API request was rejected or returned an unexpected status",
                );
                if let Ok(value) = decoded
                    && let Some(errors) = value.get("errors").and_then(Value::as_array)
                {
                    if (400..500).contains(&status)
                        && !errors.is_empty()
                        && errors.iter().all(|e| {
                            e.get("code")
                                .and_then(Value::as_number)
                                .is_some_and(|n| n.to_string().bytes().all(|c| c.is_ascii_digit()))
                                && e.get("message").is_some_and(Value::is_string)
                        })
                    {
                        progress.lock().expect("write state").0 = not_written;
                    }
                    error.api_errors = errors.clone();
                }
                return Err(error);
            }
            decoded.map_err(|_| Error::new("invalid_response", "API response is not valid JSON"))
        });
        let result = tokio::select! {
            result = &mut task => result.unwrap_or_else(|_| Err(Error::new("network", "Communication task failed"))),
            signal = tokio::signal::ctrl_c() => {
                task.abort();
                let _ = task.await;
                Err(match signal { Ok(()) => Error::new("interrupted", "Operation interrupted"), Err(_) => Error::new("terminal", "Cannot receive interrupt signals") })
            }
        };
        result.map_err(|mut error| {
            let (outcome, status) = *state.lock().expect("write state");
            error.http_status = status;
            error.with_write_outcome(operation, outcome)
        })
    }
    pub(crate) fn issue_url(&self, issue_key: &str) -> String {
        issue_url(&self.origin, issue_key)
    }
    pub(crate) async fn get(&self, segments: &[&str]) -> Result<Value, Error> {
        self.get_query(segments, &[]).await
    }
    pub(crate) async fn get_query(
        &self,
        segments: &[&str],
        query: &[(&str, String)],
    ) -> Result<Value, Error> {
        // Url silently drops dot segments; never let a project change the endpoint.
        if segments
            .iter()
            .any(|segment| matches!(*segment, "" | "." | ".."))
        {
            return Err(Error::new(
                "input",
                "Project cannot be an empty or dot path segment",
            ));
        }
        let mut url = self.origin.clone();
        url.path_segments_mut()
            .map_err(|_| Error::new("config", "Cannot construct API URL"))?
            .clear()
            .extend(["api", "v2"])
            .extend(segments);
        let mut http_status = None;
        let request = async {
            let response = self
                .client
                .get(url)
                .query(query)
                .header("Backlog-API-Key", self.key.clone())
                .send()
                .await
                .map_err(|_| Error::new("network", "HTTPS request failed or timed out"))?;
            let status = response.status().as_u16();
            http_status = Some(status);
            let body = response
                .bytes()
                .await
                .map_err(|_| Error::new("network", "Response body failed or timed out"))?;
            let decoded = serde_json::from_slice::<&RawValue>(&body)
                // Keep serde_json's default nesting limit while decoding raw containers.
                .and_then(|raw| decode_json(raw, 128));
            if status != 200 {
                let kind = match status {
                    401 => "auth",
                    403 => "permission",
                    404 => "not_found",
                    429 => "rate_limit",
                    _ => "api",
                };
                let mut error = Error::new(kind, "API request was rejected");
                if let Ok(value) = decoded {
                    error.api_errors = value
                        .get("errors")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default();
                }
                return Err(error);
            }
            let value = decoded
                .map_err(|_| Error::new("invalid_response", "API response is not valid JSON"))?;
            Ok(value)
        };
        let result = tokio::select! {
            result = request => result,
            signal = tokio::signal::ctrl_c() => match signal {
                Ok(()) => Err(Error::new("interrupted", "Operation interrupted")),
                Err(_) => Err(Error::new("terminal", "Cannot receive interrupt signals")),
            },
        };
        result.map_err(|mut error| {
            error.http_status = http_status;
            error
        })
    }
}

pub(crate) fn issue_url(origin: &reqwest::Url, issue_key: &str) -> String {
    let mut url = origin.clone();
    url.path_segments_mut()
        .expect("validated HTTPS origin")
        .clear()
        .extend(["view", issue_key]);
    url.into()
}
