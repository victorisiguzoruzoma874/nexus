use anyhow::Context;
use sqlx::postgres::PgPoolOptions;
use std::net::SocketAddr;
use std::sync::Arc;
use std::path::PathBuf;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use nexuscare_backend::utils::AppConfig;
use nexuscare_backend::routes;
use nexuscare_backend::repositories::EmailOutboxRepository;
use nexuscare_backend::services::{EmailOutboxService, EmailOutboxWorker, NotificationService};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env file (prefer project root, fallback to CWD)
    let manifest_env = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".env");
    if let Err(err) = dotenvy::from_path_override(&manifest_env) {
        if let Err(err2) = dotenvy::from_filename_override(".env") {
            tracing::warn!(
                "Failed to load .env from {}: {}; also failed from CWD: {}",
                manifest_env.display(),
                err,
                err2
            );
        }
    }

    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "nexuscare_backend=debug,tower_http=debug".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();


    // Load configuration
    let cfg = AppConfig::from_env().context("Failed to load configuration")?;

    // Connect to database
    let pool = PgPoolOptions::new()
        .max_connections(cfg.database.max_connections)
        .connect(&cfg.database.url)
        .await
        .context("Failed to connect to PostgreSQL")?;

    // Run migrations
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("Failed to run database migrations")?;

    tracing::info!("Database migrations applied successfully");

    let notification_service = Arc::new(NotificationService::new());
    let email_outbox_repo = Arc::new(EmailOutboxRepository::new(pool.clone()));
    let email_outbox_service = Arc::new(EmailOutboxService::new(
        email_outbox_repo,
        notification_service.clone(),
    ));

    let worker = EmailOutboxWorker::new(email_outbox_service.clone());
    tokio::spawn(worker.run());

    // Build the application router
    let app = routes::create_router(
        pool.clone(),
        notification_service,
        email_outbox_service,
    );

    let addr: SocketAddr = format!("{}:{}", cfg.server.host, cfg.server.port)
        .parse()
        .context("Invalid server address")?;

    tracing::info!("NexusCare backend listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
