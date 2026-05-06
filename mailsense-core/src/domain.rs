use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub id: Option<Uuid>,
    pub filename: String,
    pub mime_type: String,
    pub data: Vec<u8>,
    #[serde(default)]
    pub is_encrypted: bool,
    #[serde(default)]
    pub is_decrypted: bool,
    #[serde(default)]
    pub decryption_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailMessage {
    pub id: Option<Uuid>,
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

#[async_trait]
pub trait StorageProvider: Send + Sync {
    /// Check if an email has already been processed by its Message-ID.
    /// This now checks for existence in the email_documents table.
    async fn is_email_processed(&self, message_id: &str) -> anyhow::Result<bool>;

    /// Get an email by its Message-ID.
    async fn get_email_by_id(&self, message_id: &str) -> anyhow::Result<Option<EmailMessage>>;

    /// Get all attachments for a specific Message-ID.
    /// Warning: This fetches full binary data for all attachments.
    async fn get_attachments_by_message_id(
        &self,
        message_id: &str,
    ) -> anyhow::Result<Vec<Attachment>>;

    /// Get metadata (no binary data) for all attachments of a specific Message-ID.
    async fn get_attachment_metadata_by_message_id(
        &self,
        message_id: &str,
    ) -> anyhow::Result<Vec<Attachment>>;

    /// Get a specific attachment by its unique internal ID.
    async fn get_attachment_by_id(&self, attachment_id: Uuid)
    -> anyhow::Result<Option<Attachment>>;

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
