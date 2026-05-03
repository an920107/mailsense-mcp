use async_imap::Session;
use async_trait::async_trait;
use mailsense_core::config::ImapConfig;
use mailsense_core::domain::{EmailMessage, EmailProvider};
use native_tls::TlsConnector;
use tokio::net::TcpStream;
use futures::StreamExt;

pub struct ImapClient {
    config: ImapConfig,
}

impl ImapClient {
    pub fn new(config: ImapConfig) -> Self {
        Self { config }
    }

    async fn connect(&self) -> anyhow::Result<Session<tokio_native_tls::TlsStream<TcpStream>>> {
        let addr = format!("{}:{}", self.config.host, self.config.port);
        let tcp_stream = TcpStream::connect(&addr).await?;

        let tls_stream = if self.config.tls_enabled {
            let connector = TlsConnector::builder().build()?;
            let tokio_connector = tokio_native_tls::TlsConnector::from(connector);
            tokio_connector.connect(&self.config.host, tcp_stream).await?
        } else {
            return Err(anyhow::anyhow!("Non-TLS connections are not yet implemented"));
        };

        let client = async_imap::Client::new(tls_stream);
        let mut session = client
            .login(&self.config.username, &self.config.password)
            .await
            .map_err(|(e, _)| e)?;
        
        session.select("INBOX").await?;
        Ok(session)
    }
}

#[async_trait]
impl EmailProvider for ImapClient {
    async fn fetch_recent(&self, limit: u32) -> anyhow::Result<Vec<EmailMessage>> {
        let mut session = self.connect().await?;
        
        let search_results = session.search("ALL").await?;
        let total = search_results.len();
        
        if total == 0 {
            return Ok(vec![]);
        }

        let start = if total > limit as usize { total - limit as usize + 1 } else { 1 };
        let end = total;

        let fetch_range = format!("{}:{}", start, end);
        let mut messages_stream = session.fetch(fetch_range, "RFC822").await?;
        
        let mut result = Vec::new();
        while let Some(msg_result) = messages_stream.next().await {
            let msg = msg_result?;
            if let Some(body) = msg.body() {
                if let Some(parsed) = mail_parser::MessageParser::new().parse(body) {
                    let subject = parsed.subject().unwrap_or("No Subject").to_string();
                    let from = parsed.from()
                        .and_then(|f| f.as_list())
                        .and_then(|l| l.first())
                        .map(|a| a.address().unwrap_or("Unknown"))
                        .unwrap_or("Unknown")
                        .to_string();
                    let body_text = parsed.body_text(0).as_deref().unwrap_or("").to_string();
                    let date = parsed.date().map(|d| d.to_rfc3339()).unwrap_or_else(|| "Unknown".to_string());

                    result.push(EmailMessage {
                        subject,
                        from,
                        body: body_text,
                        date,
                    });
                }
            }
        }

        result.reverse();
        Ok(result)
    }
}
