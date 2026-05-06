use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub filename: String,
    pub mime_type: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailMessage {
    pub message_id: String,
    pub thread_id: Option<String>,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub subject: String,
    pub from: String,
    pub body: String,
    pub date: String,
    pub attachments: Vec<Attachment>,
    pub analysis: Option<EmailAnalysis>,
}

impl EmailMessage {
    /// Generates a structured string suitable for embedding,
    /// following the Gemini 2 recommendation: "title: {title} | text: {content}"
    pub fn to_embedding_text(&self) -> String {
        format!(
            "title: {} | text: From: {}\nBody: {}",
            self.subject, self.from, self.body
        )
    }
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

#[async_trait]
pub trait StorageProvider: Send + Sync {
    /// Check if an email has already been processed by its Message-ID.
    async fn is_email_processed(&self, message_id: &str) -> anyhow::Result<bool>;

    /// Mark an email as processed.
    async fn mark_email_processed(&self, message_id: &str) -> anyhow::Result<()>;

    /// Get an email by its Message-ID.
    async fn get_email_by_id(&self, message_id: &str) -> anyhow::Result<Option<EmailMessage>>;

    /// Store a processed email document with its embedding and threading info.
    async fn store_email_document(
        &self,
        email: &EmailMessage,
        thread_id: &str,
        embedding: Option<Vec<f32>>,
        analysis: Option<EmailAnalysis>,
    ) -> anyhow::Result<()>;

    /// Perform a hybrid search using vector similarity and keyword matching.
    async fn hybrid_search(
        &self,
        query_text: &str,
        query_embedding: Option<Vec<f32>>,
        intent: Option<EmailIntent>,
        limit: u32,
    ) -> anyhow::Result<Vec<EmailMessage>>;

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

#[async_trait]
pub trait EmailProvider: Send + Sync {
    /// Fetch the latest `limit` emails from the provider.
    async fn fetch_recent(&self, limit: u32) -> anyhow::Result<Vec<EmailMessage>>;

    /// Fetch emails received since the given timestamp.
    async fn fetch_since(&self, since: DateTime<Utc>) -> anyhow::Result<Vec<EmailMessage>>;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EmailIntent {
    ActionRequired,
    FYI,
    Update,
    Spam,
}

impl EmailIntent {
    pub fn as_str(&self) -> &'static str {
        match self {
            EmailIntent::ActionRequired => "ActionRequired",
            EmailIntent::FYI => "FYI",
            EmailIntent::Update => "Update",
            EmailIntent::Spam => "Spam",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum PasswordComponent {
    #[serde(rename = "ID")]
    Id {
        #[serde(default = "default_operation")]
        operation: String, // "Full", "First", "Last"
        length: Option<usize>,
    },
    #[serde(rename = "Bday")]
    Bday {
        #[serde(default = "default_bday_format")]
        format: String, // "YYYYMMDD", "MMDD", "YYMMDD", "YYMM", "MINGUO"
    },
    #[serde(rename = "Literal")]
    Literal { value: String },
}

fn default_operation() -> String {
    "Full".to_string()
}

fn default_bday_format() -> String {
    "YYYYMMDD".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailAnalysis {
    pub intent: EmailIntent,
    pub tags: Vec<String>,
    pub summary: String,
    pub extracted_deadlines: Vec<String>,
    pub password_recipes: Option<Vec<Vec<PasswordComponent>>>,
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Analyzes an email to categorize it, summarize it, and extract potential deadlines and password recipes.
    async fn analyze_email(&self, email: &EmailMessage) -> anyhow::Result<EmailAnalysis>;

    /// Generates a vector embedding for the given email (text + attachments).
    async fn generate_embedding(&self, email: &EmailMessage) -> anyhow::Result<Vec<f32>>;

    /// Generates a vector embedding for a raw query string.
    async fn generate_query_embedding(&self, query: &str) -> anyhow::Result<Vec<f32>>;
}
