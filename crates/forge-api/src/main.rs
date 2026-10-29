use forge_api::MIGRATOR;
use forge_api::app::{AppState, router};
use forge_api::config::Config;
use forge_api::github::app_auth::load_private_key;
use forge_api::github::{GitHubClient, OAuthApp};
use forge_api::worker::Worker;
use sqlx::postgres::PgPoolOptions;
use std::net::SocketAddr;
use std::process::ExitCode;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

const USAGE: &str = "usage: forge-api <serve|worker|migrate>";

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    let command = std::env::args().nth(1).unwrap_or_default();
    if !matches!(command.as_str(), "serve" | "worker" | "migrate") {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    let config = match Config::from_env() {
        Ok(config) => config,
        Err(e) => {
            eprintln!("configuration error: {e}");
            return ExitCode::from(2);
        }
    };
    let pool = match PgPoolOptions::new().max_connections(10).connect(&config.database_url).await {
        Ok(pool) => pool,
        Err(e) => {
            eprintln!("cannot connect to the database: {e}");
            return ExitCode::FAILURE;
        }
    };
    let result = match command.as_str() {
        "migrate" => MIGRATOR.run(&pool).await.map_err(|e| e.to_string()),
        "worker" => {
            let shutdown = CancellationToken::new();
            let token = shutdown.clone();
            tokio::spawn(async move {
                shutdown_signal().await;
                token.cancel();
            });
            tracing::info!("forge-api worker started");
            Worker::default().run(&pool, shutdown).await;
            Ok(())
        }
        _ => serve(config, pool).await,
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

async fn serve(config: Config, pool: sqlx::PgPool) -> Result<(), String> {
    if config.migrate_on_start {
        MIGRATOR.run(&pool).await.map_err(|e| e.to_string())?;
    }
    let listener = tokio::net::TcpListener::bind(config.bind_addr).await.map_err(|e| e.to_string())?;
    tracing::info!(addr = %config.bind_addr, "forge-api listening");
    let key = load_private_key(config.github_app_private_key.as_deref(), config.github_app_private_key_file.as_deref())?;
    let oauth = OAuthApp {
        oauth_url: config.github_oauth_url.clone(),
        client_id: config.github_client_id.clone(),
        client_secret: config.github_client_secret.clone(),
    };
    let github = GitHubClient::new(&config.github_api_url, &config.github_app_id, key, Arc::new(chrono::Utc::now)).with_oauth(oauth);
    let state = AppState { pool, config: Arc::new(config), github: Arc::new(github), clock: Arc::new(chrono::Utc::now) };
    let app = router(state);
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|e| e.to_string())
}

/// Ctrl+C, and SIGTERM from `docker stop` on Unix.
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = term.recv() => {}
        }
    }
    #[cfg(not(unix))]
    tokio::signal::ctrl_c().await.ok();
}
