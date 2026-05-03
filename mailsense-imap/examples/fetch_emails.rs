use mailsense_core::config::Config;
use mailsense_core::domain::EmailProvider;
use mailsense_imap::ImapClient;
use tracing::{info, error};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing for the example
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting IMAP fetch example...");

    // 1. Load configuration from .env
    let config = match Config::load() {
        Ok(cfg) => cfg,
        Err(e) => {
            error!("Failed to load configuration: {}. Make sure .env file exists and is populated.", e);
            return Err(e);
        }
    };

    // 2. Initialize the ImapClient
    info!("Connecting to IMAP host: {}:{}", config.imap.host, config.imap.port);
    let client = ImapClient::new(config.imap);

    // 3. Fetch recent emails (e.g., last 5)
    let limit = 5;
    match client.fetch_recent(limit).await {
        Ok(messages) => {
            info!("Successfully fetched {} messages.", messages.len());
            
            for (i, msg) in messages.iter().enumerate() {
                println!("\n[Email {}]", i + 1);
                println!("Subject : {}", msg.subject);
                println!("From    : {}", msg.from);
                println!("Date    : {}", msg.date);
                println!("Preview : {}", if msg.body.len() > 100 { format!("{}...", &msg.body[..100]) } else { msg.body.clone() });
                println!("--------------------------------------------------");
            }
        }
        Err(e) => {
            error!("Error fetching emails: {:?}", e);
            return Err(e);
        }
    }

    Ok(())
}
