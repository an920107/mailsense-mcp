use serde::Deserialize;
use anyhow::Context;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub database_url: String,
    pub log_level: String,
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
        let log_level = std::env::var("LOG_LEVEL")
            .unwrap_or_else(|_| "info".to_string());

        Ok(Self {
            database_url,
            log_level,
        })
    }

    /// Internal helper for testing configuration parsing without modifying actual env vars
    #[cfg(test)]
    fn from_map(map: std::collections::HashMap<String, String>) -> anyhow::Result<Self> {
        let database_url = map.get("DATABASE_URL")
            .cloned()
            .context("DATABASE_URL must be set")?;
        let log_level = map.get("LOG_LEVEL")
            .cloned()
            .unwrap_or_else(|| "info".to_string());

        Ok(Self {
            database_url,
            log_level,
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
        map.insert("DATABASE_URL".to_string(), "postgres://test:test@localhost/test".to_string());
        map.insert("LOG_LEVEL".to_string(), "debug".to_string());

        let config = Config::from_map(map).expect("Failed to load config");
        assert_eq!(config.database_url, "postgres://test:test@localhost/test");
        assert_eq!(config.log_level, "debug");
    }
}
