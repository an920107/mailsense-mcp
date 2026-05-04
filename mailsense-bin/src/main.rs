use mailsense_core::config::Config;
use mailsense_mcp::server::McpServer;
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
    tracing::info!(
        "Configuration loaded. Database URL: {}",
        config.database_url
    );

    // 3. Initialize and Run MCP Server
    let server = McpServer::new("MailSense-MCP", "0.1.0");
    server.run().await?;

    Ok(())
}
