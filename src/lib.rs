//! Shared configuration, API access and safe display for the Backlog CLI and TUI.
pub mod api;
pub mod config;
pub mod safety;

use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Error {
    pub kind: &'static str,
    pub message: String,
    pub http_status: Option<u16>,
    pub api_errors: Vec<Value>,
}
impl Error {
    pub fn new(kind: &'static str, message: &str) -> Self {
        Self {
            kind,
            message: message.into(),
            http_status: None,
            api_errors: vec![],
        }
    }
    pub fn exit_code(&self) -> u8 {
        match self.kind {
            "input" => 2,
            "interrupted" => 130,
            _ => 1,
        }
    }
}
