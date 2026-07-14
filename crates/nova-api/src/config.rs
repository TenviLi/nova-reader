use serde::Deserialize;

/// Application configuration loaded from environment variables.
#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    // Server
    pub host: String,
    pub port: u16,
    pub env: Environment,

    // Database
    pub database_url: String,

    // Redis
    pub redis_url: String,

    // Qdrant
    pub qdrant_url: String,

    // Neo4j
    pub neo4j_uri: String,
    pub neo4j_user: String,
    pub neo4j_password: String,

    // Meilisearch
    pub meili_url: String,
    pub meili_master_key: String,

    // JWT
    pub jwt_secret: String,
    pub jwt_expiry: String,

    // AI
    pub deepseek_api_key: String,
    pub deepseek_base_url: String,
    pub deepseek_model: String,

    // Embedding
    pub embedding_endpoint: String,
    pub embedding_model: String,
    pub embedding_api_key: String,
    pub embedding_dimensions: usize,

    // Reranker
    pub reranker_endpoint: String,
    pub reranker_model: String,

    // File system
    pub inbox_dir: String,
    pub library_dir: String,
    pub data_dir: String,
    pub debounce_ms: u64,

    // Worker
    pub worker_concurrency: usize,

    // TLS (optional)
    pub tls_cert_path: Option<String>,
    pub tls_key_path: Option<String>,

    // CORS
    pub cors_origins: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    Development,
    Production,
}

impl AppConfig {
    /// Load configuration from environment variables.
    pub fn from_env() -> anyhow::Result<Self> {
        let config = Self {
            host: std::env::var("NOVA_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            port: std::env::var("NOVA_PORT")
                .unwrap_or_else(|_| "3000".to_string())
                .parse()?,
            env: match std::env::var("NOVA_ENV")
                .unwrap_or_else(|_| "development".to_string())
                .as_str()
            {
                "production" => Environment::Production,
                _ => Environment::Development,
            },
            database_url: std::env::var("DATABASE_URL").unwrap_or_else(|_| {
                "postgres://nova:nova_secret_2024@localhost:5432/nova_reader".to_string()
            }),
            redis_url: std::env::var("REDIS_URL")
                .unwrap_or_else(|_| "redis://localhost:6379".to_string()),
            qdrant_url: std::env::var("QDRANT_URL")
                .unwrap_or_else(|_| "http://localhost:6333".to_string()),
            neo4j_uri: std::env::var("NEO4J_URI")
                .unwrap_or_else(|_| "bolt://localhost:7687".to_string()),
            neo4j_user: std::env::var("NEO4J_USER").unwrap_or_else(|_| "neo4j".to_string()),
            neo4j_password: std::env::var("NEO4J_PASSWORD")
                .unwrap_or_else(|_| "nova_graph_2024".to_string()),
            meili_url: std::env::var("MEILI_URL")
                .unwrap_or_else(|_| "http://localhost:7700".to_string()),
            meili_master_key: std::env::var("MEILI_MASTER_KEY")
                .unwrap_or_else(|_| "nova_search_key_2024".to_string()),
            jwt_secret: std::env::var("NOVA_JWT_SECRET")
                .unwrap_or_else(|_| "dev-secret-change-in-production".to_string()),
            jwt_expiry: std::env::var("NOVA_JWT_EXPIRY").unwrap_or_else(|_| "7d".to_string()),
            deepseek_api_key: std::env::var("DEEPSEEK_API_KEY").unwrap_or_else(|_| String::new()),
            deepseek_base_url: std::env::var("DEEPSEEK_BASE_URL")
                .unwrap_or_else(|_| "https://api.deepseek.com".to_string()),
            deepseek_model: std::env::var("DEEPSEEK_MODEL")
                .unwrap_or_else(|_| "deepseek-chat".to_string()),
            embedding_endpoint: std::env::var("EMBEDDING_ENDPOINT")
                .unwrap_or_else(|_| "https://ai.gitee.com".to_string()),
            embedding_model: std::env::var("EMBEDDING_MODEL")
                .unwrap_or_else(|_| "Qwen3-Embedding-4B".to_string()),
            embedding_api_key: std::env::var("EMBEDDING_API_KEY").unwrap_or_else(|_| String::new()),
            embedding_dimensions: std::env::var("EMBEDDING_DIMENSIONS")
                .unwrap_or_else(|_| "2560".to_string())
                .parse()
                .unwrap_or(2560),
            reranker_endpoint: std::env::var("RERANKER_ENDPOINT")
                .unwrap_or_else(|_| "http://127.0.0.1:8000".to_string()),
            reranker_model: std::env::var("RERANKER_MODEL")
                .unwrap_or_else(|_| "Qwen3-Reranker-4B".to_string()),
            inbox_dir: std::env::var("NOVA_INBOX_DIR")
                .unwrap_or_else(|_| "./data/inbox".to_string()),
            library_dir: std::env::var("NOVA_LIBRARY_DIR")
                .unwrap_or_else(|_| "./data/library".to_string()),
            data_dir: std::env::var("NOVA_DATA_DIR").unwrap_or_else(|_| "./data".to_string()),
            debounce_ms: std::env::var("NOVA_DEBOUNCE_MS")
                .unwrap_or_else(|_| "500".to_string())
                .parse()?,
            worker_concurrency: std::env::var("NOVA_WORKER_CONCURRENCY")
                .unwrap_or_else(|_| "8".to_string())
                .parse()?,
            tls_cert_path: std::env::var("NOVA_TLS_CERT").ok(),
            tls_key_path: std::env::var("NOVA_TLS_KEY").ok(),
            cors_origins: std::env::var("NOVA_CORS_ORIGINS")
                .unwrap_or_else(|_| "http://localhost:5173,http://localhost:4173".to_string())
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
        };

        config.validate()?;
        Ok(config)
    }

    /// Validate security-critical settings.
    fn validate(&self) -> anyhow::Result<()> {
        if self.env == Environment::Production {
            if self.jwt_secret == "dev-secret-change-in-production" {
                anyhow::bail!(
                    "FATAL: NOVA_JWT_SECRET must be set to a unique secret in production. \
                     Refusing to start with the default development secret."
                );
            }
            if self.jwt_secret.len() < 32 {
                anyhow::bail!(
                    "FATAL: NOVA_JWT_SECRET must be at least 32 characters in production."
                );
            }
            if self.database_url.contains("nova_secret_2024") {
                anyhow::bail!(
                    "FATAL: DATABASE_URL must not use the default development credentials in production."
                );
            }
            if self.neo4j_password == "nova_graph_2024" {
                anyhow::bail!(
                    "FATAL: NEO4J_PASSWORD must not use the default development credentials in production."
                );
            }
            if self.meili_master_key == "nova_search_key_2024" {
                anyhow::bail!(
                    "FATAL: MEILI_MASTER_KEY must not use the default development key in production."
                );
            }
        }
        match (&self.tls_cert_path, &self.tls_key_path) {
            (Some(_), Some(_)) | (None, None) => {}
            _ => anyhow::bail!("NOVA_TLS_CERT and NOVA_TLS_KEY must be set together"),
        }
        Ok(())
    }

    pub fn is_production(&self) -> bool {
        self.env == Environment::Production
    }

    pub fn jwt_access_ttl(&self) -> anyhow::Result<chrono::Duration> {
        parse_duration(&self.jwt_expiry)
    }

    pub fn tls_configured(&self) -> bool {
        self.tls_cert_path.is_some() && self.tls_key_path.is_some()
    }
}

fn parse_duration(value: &str) -> anyhow::Result<chrono::Duration> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        anyhow::bail!("duration cannot be empty");
    }

    let (amount, unit) = trimmed.split_at(trimmed.len().saturating_sub(1));
    let (amount, unit) = if unit.chars().all(|c| c.is_ascii_alphabetic()) {
        (amount, unit)
    } else {
        (trimmed, "s")
    };
    let amount: i64 = amount.parse()?;
    if amount <= 0 {
        anyhow::bail!("duration must be positive");
    }

    match unit {
        "s" => Ok(chrono::Duration::seconds(amount)),
        "m" => Ok(chrono::Duration::minutes(amount)),
        "h" => Ok(chrono::Duration::hours(amount)),
        "d" => Ok(chrono::Duration::days(amount)),
        _ => anyhow::bail!("unsupported duration unit: {unit}"),
    }
}
