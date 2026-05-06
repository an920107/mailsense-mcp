use anyhow::Context;
use chrono::{DateTime, Utc};
use futures::StreamExt;
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
    let gemini_cfg = config
        .gemini
        .as_ref()
        .context("Gemini configuration is missing in .env (GEMINI_API_KEY, etc.)")?;
    let llm = Arc::new(GeminiClient::new(
        gemini_cfg.api_key.clone(),
        gemini_cfg.model.clone(),
        gemini_cfg.embedding_model.clone(),
        Some(gemini_cfg.base_url.clone()),
        gemini_cfg.max_attachment_size,
        gemini_cfg.max_multimodal_parts,
    ));

    let storage = Arc::new(PgStorage::connect(&config.database_url).await?);

    let email_provider = Arc::new(ImapClient::new(config.imap.clone()));

    // 4. Spawn Background Ingestion Worker (Runs every 15 minutes)
    let worker_llm = llm.clone();
    let worker_storage = storage.clone();
    let worker_email = email_provider.clone();
    let worker_personal_config = Arc::new(config.personal.clone());

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
                worker_email.clone(),
                worker_llm.clone(),
                worker_storage.clone(),
                worker_personal_config.clone(),
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
    email_provider: Arc<dyn EmailProvider>,
    llm: Arc<dyn LlmProvider>,
    storage: Arc<dyn StorageProvider>,
    personal_config: Arc<Option<mailsense_core::config::PersonalConfig>>,
    since: DateTime<Utc>,
) -> anyhow::Result<usize> {
    let emails = email_provider.fetch_since(since).await?;
    process_emails(emails, llm, storage, personal_config).await
}

async fn process_emails(
    emails: Vec<mailsense_core::domain::EmailMessage>,
    llm: Arc<dyn LlmProvider>,
    storage: Arc<dyn StorageProvider>,
    personal_config: Arc<Option<mailsense_core::config::PersonalConfig>>,
) -> anyhow::Result<usize> {
    let processed_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let concurrency_limit = 5;

    let stream = futures::stream::iter(emails).map(|mut email| {
        let llm = llm.clone();
        let storage = storage.clone();
        let personal_config = personal_config.clone();
        let processed_count = processed_count.clone();

        async move {
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

                // 2.5 Attempt PDF decryption if personal config is present
                if let Some(cfg) = personal_config.as_ref() {
                    let recipes = analysis.as_ref().and_then(|a| a.password_recipes.as_ref());
                    let builder = mailsense_core::password::PasswordPoolBuilder::new(cfg);
                    let pool = builder.build(&email, recipes);

                    for attachment in &mut email.attachments {
                        if attachment.mime_type.to_lowercase() == "application/pdf" {
                            tracing::info!(
                                "Attempting to decrypt PDF attachment: {}",
                                attachment.filename
                            );
                            attachment.is_encrypted = true;

                            // We do a quick check if it's actually encrypted by trying to load it
                            if let Ok(doc) = lopdf::Document::load_mem(&attachment.data)
                                && !doc.is_encrypted()
                            {
                                attachment.is_encrypted = false;
                                attachment.is_decrypted = false; // It's not decrypted, it was never encrypted
                                continue;
                            }
                            match mailsense_core::pdf::decrypt_pdf_with_timeout(
                                &attachment.data,
                                &pool,
                            )
                            .await
                            {
                                Ok(Some(decrypted_bytes)) => {
                                    tracing::info!(
                                        "Successfully decrypted {}",
                                        attachment.filename
                                    );
                                    attachment.data = decrypted_bytes;
                                    attachment.is_decrypted = true;
                                }
                                Ok(None) => {
                                    tracing::warn!(
                                        "Failed to find correct password for {}",
                                        attachment.filename
                                    );
                                    attachment.decryption_error =
                                        Some("Password not found in pool".to_string());
                                }
                                Err(e) => {
                                    tracing::error!(
                                        "Decryption error for {}: {}",
                                        attachment.filename,
                                        e
                                    );
                                    attachment.decryption_error = Some(e.to_string());
                                }
                            }
                        }
                    }
                }

                // 3. Store document
                let thread_id = email
                    .thread_id
                    .as_deref()
                    .unwrap_or(&email.message_id)
                    .to_string();

                storage
                    .store_email_document(&email, &thread_id, Some(embedding), analysis)
                    .await?;

                processed_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
            Ok::<(), anyhow::Error>(())
        }
    });

    // Execute concurrently
    let results: Vec<anyhow::Result<()>> =
        stream.buffer_unordered(concurrency_limit).collect().await;

    // Check for any critical errors
    for res in results {
        res?;
    }

    Ok(processed_count.load(std::sync::atomic::Ordering::SeqCst))
}
