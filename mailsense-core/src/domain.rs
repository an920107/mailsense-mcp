use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailMessage {
    pub subject: String,
    pub from: String,
    pub body: String,
    pub date: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

impl TaskStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskStatus::Pending => "Pending",
            TaskStatus::InProgress => "InProgress",
            TaskStatus::Completed => "Completed",
            TaskStatus::Failed => "Failed",
        }
    }
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    pub task_type: String,
    pub status: TaskStatus,
    pub payload: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessedEmail {
    pub id: Uuid,
    pub message_id: String,
    pub processed_at: DateTime<Utc>,
}

#[async_trait]
pub trait EmailProvider: Send + Sync {
    async fn fetch_recent(&self, limit: u32) -> anyhow::Result<Vec<EmailMessage>>;
}

#[async_trait]
pub trait StorageProvider: Send + Sync {
    /// Check if an email has already been processed by its Message-ID.
    async fn is_email_processed(&self, message_id: &str) -> anyhow::Result<bool>;

    /// Mark an email as processed.
    async fn mark_email_processed(&self, message_id: &str) -> anyhow::Result<()>;

    /// Enqueue a new background task.
    async fn enqueue_task(
        &self,
        task_type: &str,
        payload: serde_json::Value,
    ) -> anyhow::Result<Task>;

    /// Get a pending task and mark it as InProgress.
    async fn pick_next_task(&self) -> anyhow::Result<Option<Task>>;

    /// Update the status of a task.
    async fn update_task_status(&self, id: Uuid, status: TaskStatus) -> anyhow::Result<()>;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EmailIntent {
    ActionRequired,
    FYI,
    Update,
    Spam,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailAnalysis {
    pub intent: EmailIntent,
    pub tags: Vec<String>,
    pub summary: String,
    pub extracted_deadlines: Vec<DateTime<Utc>>,
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Analyzes an email to categorize it, summarize it, and extract potential deadlines.
    async fn analyze_email(&self, email: &EmailMessage) -> anyhow::Result<EmailAnalysis>;
}
