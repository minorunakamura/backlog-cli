//! Shared configuration, API access and safe display for the Backlog CLI and TUI.
pub mod api;
pub mod body;
pub mod comment;
pub mod config;
pub mod editor;
pub mod safety;
pub mod service;
pub mod tui;
mod tui_datetime;
mod tui_text;

use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WriteOutcome {
    NotCreated,
    Unknown,
    Created,
    NotUpdated,
    Updated,
}
/// Creation callers retain the same source and serialized outcome vocabulary.
pub use WriteOutcome as CreateOutcome;

/// In-memory creation certainty retained when the caller aborts and joins its task.
#[derive(Clone)]
pub struct CreateProgress(std::sync::Arc<std::sync::Mutex<(WriteOutcome, Option<u16>)>>);
impl Default for CreateProgress {
    fn default() -> Self {
        Self::new(WriteOutcome::NotCreated)
    }
}
impl CreateProgress {
    pub(crate) fn new(outcome: WriteOutcome) -> Self {
        Self(std::sync::Arc::new(std::sync::Mutex::new((outcome, None))))
    }
    pub fn snapshot(&self) -> (WriteOutcome, Option<u16>) {
        *self.0.lock().expect("write state")
    }
    pub(crate) fn record(&self, outcome: WriteOutcome, status: Option<u16>) {
        *self.0.lock().expect("write state") = (outcome, status);
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct WriteResult {
    pub operation: &'static str,
    pub outcome: WriteOutcome,
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
    pub fn with_create_outcome(self, outcome: CreateOutcome) -> Self {
        self.with_write_outcome("issue.create", outcome)
    }
    pub fn with_update_outcome(self, outcome: WriteOutcome) -> Self {
        self.with_write_outcome("issue.update", outcome)
    }
    pub fn with_write_outcome(mut self, operation: &'static str, outcome: WriteOutcome) -> Self {
        self.write.get_or_insert(WriteResult { operation, outcome });
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
