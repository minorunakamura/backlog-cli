use crate::{Error, config::Config};
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
    url: reqwest::Url,
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
        let url = config
            .origin
            .join("/api/v2/projects")
            .map_err(|_| Error::new("config", "Cannot construct projects URL"))?;
        Ok(Self { client, url, key })
    }
    pub async fn project_list(&self) -> Result<Value, Error> {
        let mut http_status = None;
        let request = async {
            let response = self
                .client
                .get(self.url.clone())
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
                let mut error = Error::new(kind, "Projects request was rejected");
                if let Ok(value) = decoded {
                    error.api_errors = value
                        .get("errors")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default();
                }
                return Err(error);
            }
            let value = decoded.map_err(|_| {
                Error::new("invalid_response", "Projects response is not valid JSON")
            })?;
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
                return Err(Error::new(
                    "invalid_response",
                    "Projects response has invalid required fields",
                ));
            }
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
