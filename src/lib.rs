//! Shared configuration, API access and safe display for the Backlog CLI and TUI.
pub mod api;
pub mod body;
pub mod config;
pub mod safety;
pub mod service;

use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CreateOutcome {
    NotCreated,
    Unknown,
    Created,
}
#[derive(Clone, Copy, Debug, Serialize)]
pub struct WriteResult {
    pub operation: &'static str,
    pub outcome: CreateOutcome,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Error {
    pub kind: &'static str,
    pub message: String,
    pub http_status: Option<u16>,
    pub api_errors: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub write: Option<WriteResult>,
}
impl Error {
    pub fn new(kind: &'static str, message: &str) -> Self {
        Self {
            kind,
            message: message.into(),
            http_status: None,
            api_errors: vec![],
            write: None,
        }
    }
    /// Add fallback metadata without lowering certainty already recorded by communication.
    pub fn with_create_outcome(mut self, outcome: CreateOutcome) -> Self {
        self.write.get_or_insert(WriteResult {
            operation: "issue.create",
            outcome,
        });
        self
    }
    pub fn exit_code(&self) -> u8 {
        match self.kind {
            "input" => 2,
            "interrupted" => 130,
            _ => 1,
        }
    }
}
