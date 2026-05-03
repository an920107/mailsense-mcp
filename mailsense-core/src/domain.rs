use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailMessage {
    pub subject: String,
    pub from: String,
    pub body: String,
    pub date: String,
}

#[async_trait]
pub trait EmailProvider: Send + Sync {
    async fn fetch_recent(&self, limit: u32) -> anyhow::Result<Vec<EmailMessage>>;
}
