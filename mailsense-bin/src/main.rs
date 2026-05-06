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
        loop {
            tracing::info!("Starting scheduled ingestion...");
            match run_ingestion(
                worker_email.as_ref(),
                worker_llm.as_ref(),
                worker_storage.as_ref(),
            )
            .await
            {
                Ok(count) => tracing::info!("Ingestion complete. Processed {} new emails.", count),
                Err(e) => tracing::error!("Ingestion worker error: {}", e),
            }

            tracing::info!("Next ingestion in 15 minutes.");
            tokio::time::sleep(Duration::from_secs(15 * 60)).await;
        }
    });

    // 5. Initialize and Run MCP Server
    let server = McpServer::new("MailSense-MCP", "0.1.0", storage, llm);
    server.run().await?;

    Ok(())
}

async fn run_ingestion(
    email_provider: &dyn EmailProvider,
    llm: &dyn LlmProvider,
    storage: &dyn StorageProvider,
) -> anyhow::Result<usize> {
    // Fetch latest 50 emails
    let emails = email_provider.fetch_recent(50).await?;
    let mut processed_count = 0;

    for email in emails {
        if !storage.is_email_processed(&email.message_id).await? {
            tracing::info!("Processing new email: {}", email.subject);

            // 1. Generate multi-modal embedding
            let embedding = llm.generate_embedding(&email).await?;

            // 2. Store document (using surrogate logic for threading if needed)
            // For now, we use message_id as thread_id if not present to simplify
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
