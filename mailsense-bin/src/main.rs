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
        let initial_days = config.ingestion_initial_days;
        let interval_mins = config.ingestion_interval_minutes;

        loop {
            // We always fetch emails since a calculated window to ensure no loss.
            // Even if 100+ emails arrive in 15 mins, they will all be captured.
            // The is_email_processed guard in process_emails handles deduplication.
            let since = Utc::now() - Duration::from_secs(initial_days as u64 * 24 * 3600);

            tracing::info!(
                "Starting ingestion window (since {})...",
                since.format("%Y-%m-%d")
            );

            match run_ingestion_since(
                worker_email.as_ref(),
                worker_llm.as_ref(),
                worker_storage.as_ref(),
                since,
            )
            .await
            {
                Ok(count) => {
                    if count > 0 {
                        tracing::info!("Ingestion complete. Processed {} new emails.", count);
                    } else {
                        tracing::debug!("Ingestion complete. No new emails found.");
                    }
                }
                Err(e) => tracing::error!("Ingestion error: {}", e),
            }

            tracing::info!("Next ingestion in {} minutes.", interval_mins);
            tokio::time::sleep(Duration::from_secs(interval_mins * 60)).await;
        }
    });

    // 5. Initialize and Run MCP Server
    let server = McpServer::new("MailSense-MCP", env!("CARGO_PKG_VERSION"), storage, llm);
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

            // 2. Perform deep analysis
            let analysis = match llm.analyze_email(&email).await {
                Ok(a) => Some(a),
                Err(e) => {
                    tracing::warn!("Failed to analyze email {}: {}", email.message_id, e);
                    None
                }
            };

            // 3. Store document
            let thread_id = email
                .thread_id
                .as_deref()
                .unwrap_or(&email.message_id)
                .to_string();

            storage
                .store_email_document(&email, &thread_id, Some(embedding), analysis)
                .await?;

            // 4. Mark as processed
            storage.mark_email_processed(&email.message_id).await?;

            processed_count += 1;
        }
    }

    Ok(processed_count)
}
