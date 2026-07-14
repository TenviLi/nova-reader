use sqlx::PgPool;
use std::sync::Arc;

use crate::auth::jwt::JwtService;
use crate::config::AppConfig;
use crate::middleware::{
    create_ai_rate_limiter, create_per_user_rate_limiter, AiRateLimiter, PerUserRateLimiter,
};
use crate::migrations::run_database_migrations;
use crate::repo::pg_book::PgBookRepository;
use crate::repo::pg_chapter::PgChapterRepository;
use crate::repo::pg_duplicate::PgDuplicateRepository;
use crate::repo::pg_library::PgLibraryRepository;
use crate::repo::pg_user::PgUserRepository;
use crate::task_queue::TaskQueue;
use nova_graph::neo4j::Neo4jClient;

/// Shared application state accessible from all handlers via `State` extractor.
pub struct AppState {
    pub config: AppConfig,
    pub db: PgPool,
    pub redis: redis::Client,
    pub jwt: JwtService,
    pub ai_rate_limiter: AiRateLimiter,
    pub per_user_rate_limiter: Arc<PerUserRateLimiter>,
    pub http_client: reqwest::Client,
    pub neo4j: Neo4jClient,
    pub task_queue: TaskQueue,
    /// Global semaphore to limit concurrent embedding requests
    pub embedding_semaphore: Arc<tokio::sync::Semaphore>,
    pub users: PgUserRepository,
    pub books: PgBookRepository,
    pub chapters: PgChapterRepository,
    pub duplicates: PgDuplicateRepository,
    pub libraries: PgLibraryRepository,
}

impl AppState {
    pub async fn new(config: AppConfig) -> anyhow::Result<Self> {
        // Connect to PostgreSQL
        let db = sqlx::pool::PoolOptions::new()
            .max_connections(20)
            .min_connections(5)
            .acquire_timeout(std::time::Duration::from_secs(5))
            .idle_timeout(std::time::Duration::from_secs(600))
            .max_lifetime(std::time::Duration::from_secs(1800))
            .connect(&config.database_url)
            .await?;

        tracing::info!("✓ PostgreSQL connected");

        // Run pending migrations automatically. A partially migrated schema is
        // more dangerous than a failed startup because route handlers assume the
        // current schema contract.
        run_database_migrations(&db).await?;
        tracing::info!("✓ Database migrations applied");

        // Connect to Redis
        let redis = redis::Client::open(config.redis_url.as_str())?;
        let mut conn = redis.get_multiplexed_async_connection().await?;
        redis::cmd("PING").query_async::<String>(&mut conn).await?;

        tracing::info!("✓ Redis connected");

        for dir in [&config.data_dir, &config.inbox_dir, &config.library_dir] {
            tokio::fs::create_dir_all(dir).await?;
        }
        tracing::info!(
            inbox = %config.inbox_dir,
            library = %config.library_dir,
            debounce_ms = config.debounce_ms,
            "✓ File ingest directories ready"
        );

        // Shared HTTP client
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .pool_max_idle_per_host(10)
            .pool_idle_timeout(std::time::Duration::from_secs(90))
            .build()?;

        // Initialize services
        let jwt = JwtService::with_access_ttl(&config.jwt_secret, config.jwt_access_ttl()?);

        // Rate limiters
        let ai_rate_limiter = create_ai_rate_limiter(30);
        let per_user_rate_limiter = create_per_user_rate_limiter(60);

        // Repositories
        let users = PgUserRepository::new(db.clone());
        let books = PgBookRepository::new(db.clone());
        let chapters = PgChapterRepository::new(db.clone());
        let duplicates = PgDuplicateRepository::new(db.clone());
        let libraries = PgLibraryRepository::new(db.clone());

        // Neo4j graph database client
        let neo4j = Neo4jClient::new(
            &config.neo4j_uri,
            &config.neo4j_user,
            &config.neo4j_password,
        );
        tracing::info!("✓ Neo4j client initialized ({})", config.neo4j_uri);

        let task_queue = TaskQueue::new(db.clone());
        let embedding_semaphore = Arc::new(tokio::sync::Semaphore::new(
            config.worker_concurrency.max(3),
        ));

        Ok(Self {
            config,
            db,
            redis,
            jwt,
            ai_rate_limiter,
            per_user_rate_limiter,
            http_client,
            neo4j,
            task_queue,
            embedding_semaphore,
            users,
            books,
            chapters,
            duplicates,
            libraries,
        })
    }
}
