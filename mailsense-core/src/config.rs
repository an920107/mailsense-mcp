use anyhow::Context;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub database_url: String,
    pub log_level: String,
    pub imap: ImapConfig,
    pub gemini: Option<GeminiConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ImapConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub tls_enabled: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct GeminiConfig {
    pub api_key: String,
    pub model: String,
    pub base_url: String,
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        // Load .env file if it exists
        dotenvy::dotenv().ok();
        Self::load_from_env()
    }

    fn load_from_env() -> anyhow::Result<Self> {
        let database_url = std::env::var("DATABASE_URL")
            .context("DATABASE_URL must be set (e.g., postgres://user:pass@host/db)")?;
        let log_level = std::env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string());

        let imap = ImapConfig {
            host: std::env::var("IMAP_HOST").context("IMAP_HOST must be set")?,
            port: std::env::var("IMAP_PORT")
                .unwrap_or_else(|_| "993".to_string())
                .parse()
                .context("IMAP_PORT must be a valid port number")?,
            username: std::env::var("IMAP_USERNAME").context("IMAP_USERNAME must be set")?,
            password: std::env::var("IMAP_PASSWORD").context("IMAP_PASSWORD must be set")?,
            tls_enabled: std::env::var("IMAP_TLS_ENABLED")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .context("IMAP_TLS_ENABLED must be true or false")?,
        };

        let gemini = std::env::var("GEMINI_API_KEY")
            .ok()
            .map(|api_key| GeminiConfig {
                api_key,
                model: std::env::var("GEMINI_MODEL")
                    .unwrap_or_else(|_| "gemini-2.0-flash".to_string()),
                base_url: std::env::var("GEMINI_BASE_URL")
                    .unwrap_or_else(|_| "https://generativelanguage.googleapis.com".to_string()),
            });

        Ok(Self {
            database_url,
            log_level,
            imap,
            gemini,
        })
    }

    /// Internal helper for testing configuration parsing without modifying actual env vars
    #[cfg(test)]
    fn from_map(map: std::collections::HashMap<String, String>) -> anyhow::Result<Self> {
        let database_url = map
            .get("DATABASE_URL")
            .cloned()
            .context("DATABASE_URL must be set")?;
        let log_level = map
            .get("LOG_LEVEL")
            .cloned()
            .unwrap_or_else(|| "info".to_string());

        let imap = ImapConfig {
            host: map
                .get("IMAP_HOST")
                .cloned()
                .context("IMAP_HOST must be set")?,
            port: map
                .get("IMAP_PORT")
                .cloned()
                .unwrap_or_else(|| "993".to_string())
                .parse()
                .context("IMAP_PORT must be a valid port number")?,
            username: map
                .get("IMAP_USERNAME")
                .cloned()
                .context("IMAP_USERNAME must be set")?,
            password: map
                .get("IMAP_PASSWORD")
                .cloned()
                .context("IMAP_PASSWORD must be set")?,
            tls_enabled: map
                .get("IMAP_TLS_ENABLED")
                .cloned()
                .unwrap_or_else(|| "true".to_string())
                .parse()
                .context("IMAP_TLS_ENABLED must be true or false")?,
        };

        let gemini = map.get("GEMINI_API_KEY").map(|api_key| GeminiConfig {
            api_key: api_key.clone(),
            model: map
                .get("GEMINI_MODEL")
                .cloned()
                .unwrap_or_else(|| "gemini-2.0-flash".to_string()),
            base_url: map
                .get("GEMINI_BASE_URL")
                .cloned()
                .unwrap_or_else(|| "https://generativelanguage.googleapis.com".to_string()),
        });

        Ok(Self {
            database_url,
            log_level,
            imap,
            gemini,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_config_from_map() {
        let mut map = HashMap::new();
        map.insert(
            "DATABASE_URL".to_string(),
            "postgres://test:test@localhost/test".to_string(),
        );
        map.insert("LOG_LEVEL".to_string(), "debug".to_string());
        map.insert("IMAP_HOST".to_string(), "imap.example.com".to_string());
        map.insert("IMAP_PORT".to_string(), "993".to_string());
        map.insert("IMAP_USERNAME".to_string(), "user".to_string());
        map.insert("IMAP_PASSWORD".to_string(), "pass".to_string());
        map.insert("IMAP_TLS_ENABLED".to_string(), "true".to_string());
        map.insert("GEMINI_API_KEY".to_string(), "gemini-key".to_string());
        map.insert("GEMINI_MODEL".to_string(), "gemini-1.5-pro".to_string());
        map.insert(
            "GEMINI_BASE_URL".to_string(),
            "https://example.com".to_string(),
        );

        let config = Config::from_map(map).expect("Failed to load config");
        assert_eq!(config.database_url, "postgres://test:test@localhost/test");
        assert_eq!(config.log_level, "debug");
        assert_eq!(config.imap.host, "imap.example.com");
        assert_eq!(config.imap.port, 993);
        assert_eq!(config.imap.username, "user");
        assert_eq!(config.imap.password, "pass");
        assert!(config.imap.tls_enabled);

        let gemini = config.gemini.expect("Gemini config should be present");
        assert_eq!(gemini.api_key, "gemini-key");
        assert_eq!(gemini.model, "gemini-1.5-pro");
        assert_eq!(gemini.base_url, "https://example.com");
    }

    #[test]
    fn test_optional_gemini_config() {
        let mut map = HashMap::new();
        map.insert(
            "DATABASE_URL".to_string(),
            "postgres://test:test@localhost/test".to_string(),
        );
        map.insert("IMAP_HOST".to_string(), "imap.example.com".to_string());
        map.insert("IMAP_USERNAME".to_string(), "user".to_string());
        map.insert("IMAP_PASSWORD".to_string(), "pass".to_string());

        let config = Config::from_map(map).expect("Failed to load config");
        assert!(config.gemini.is_none());
    }
}
