use chrono::{DateTime, Utc};
use mailsense_core::config::Config;
use mailsense_core::domain::{EmailProvider, LlmProvider, StorageProvider};
use mailsense_core::llm::GeminiClient;
use mailsense_core::storage::PgStorage;
use mailsense_imap::client::ImapClient;
use mailsense_mcp::server::McpServer;
use std::sync::Arc;
use std::time::Duration;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Initialize Tracing (Log to stderr because stdout is for MCP)
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();

    // 2. Load Configuration
    let config = Config::load()?;
    tracing::info!("Configuration loaded.");

    // 3. Initialize Providers
    let gemini_cfg = config.gemini.as_ref().expect("Gemini config missing");
    let llm = Arc::new(GeminiClient::new(
        gemini_cfg.api_key.clone(),
        gemini_cfg.model.clone(),
        gemini_cfg.embedding_model.clone(),
        Some(gemini_cfg.base_url.clone()),
    ));

    let storage = Arc::new(PgStorage::connect(&config.database_url).await?);

    let email_provider = Arc::new(ImapClient::new(config.imap.clone()));

    // 4. Spawn Background Ingestion Worker (Runs every 15 minutes)
    let worker_llm = llm.clone();
    let worker_storage = storage.clone();
    let worker_email = email_provider.clone();

    tokio::spawn(async move {
        tracing::info!("Background ingestion worker started.");
        let mut is_first_run = true;
        let initial_days = config.ingestion_initial_days;
        let interval_mins = config.ingestion_interval_minutes;

        loop {
            if is_first_run {
                tracing::info!("Starting initial ingestion (last {} days)...", initial_days);
                let since = Utc::now() - Duration::from_secs(initial_days as u64 * 24 * 3600);
                match run_ingestion_since(
                    worker_email.as_ref(),
                    worker_llm.as_ref(),
                    worker_storage.as_ref(),
                    since,
                )
                .await
                {
                    Ok(count) => {
                        tracing::info!("Initial ingestion complete. Processed {} emails.", count);
                        is_first_run = false;
                    }
                    Err(e) => tracing::error!("Initial ingestion error: {}", e),
                }
            } else {
                tracing::info!("Starting scheduled ingestion...");
                match run_ingestion_recent(
                    worker_email.as_ref(),
                    worker_llm.as_ref(),
                    worker_storage.as_ref(),
                    50,
                )
                .await
                {
                    Ok(count) => {
                        tracing::info!(
                            "Scheduled ingestion complete. Processed {} new emails.",
                            count
                        )
                    }
                    Err(e) => tracing::error!("Ingestion worker error: {}", e),
                }
            }

            tracing::info!("Next ingestion in {} minutes.", interval_mins);
            tokio::time::sleep(Duration::from_secs(interval_mins * 60)).await;
        }
    });

    // 5. Initialize and Run MCP Server
    let server = McpServer::new("MailSense-MCP", "0.1.0", storage, llm);
    server.run().await?;

    Ok(())
}

async fn run_ingestion_since(
    email_provider: &dyn EmailProvider,
    llm: &dyn LlmProvider,
    storage: &dyn StorageProvider,
    since: DateTime<Utc>,
) -> anyhow::Result<usize> {
    let emails = email_provider.fetch_since(since).await?;
    process_emails(emails, llm, storage).await
}

async fn run_ingestion_recent(
    email_provider: &dyn EmailProvider,
    llm: &dyn LlmProvider,
    storage: &dyn StorageProvider,
    limit: u32,
) -> anyhow::Result<usize> {
    let emails = email_provider.fetch_recent(limit).await?;
    process_emails(emails, llm, storage).await
}

async fn process_emails(
    emails: Vec<mailsense_core::domain::EmailMessage>,
    llm: &dyn LlmProvider,
    storage: &dyn StorageProvider,
) -> anyhow::Result<usize> {
    let mut processed_count = 0;

    for email in emails {
        if !storage.is_email_processed(&email.message_id).await? {
            tracing::info!("Processing new email: {}", email.subject);

            // 1. Generate multi-modal embedding
            let embedding = llm.generate_embedding(&email).await?;

            // 2. Store document (using surrogate logic for threading if needed)
            let thread_id = email
                .thread_id
                .as_deref()
                .unwrap_or(&email.message_id)
                .to_string();

            storage
                .store_email_document(&email, &thread_id, Some(embedding))
                .await?;

            // 3. Mark as processed
            storage.mark_email_processed(&email.message_id).await?;

            processed_count += 1;
        }
    }

    Ok(processed_count)
}
