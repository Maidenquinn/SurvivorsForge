use forge_api::MIGRATOR;
use forge_api::app::{AppState, router};
use forge_api::config::Config;
use sqlx::postgres::PgPoolOptions;
use std::net::SocketAddr;
use std::process::ExitCode;
use std::sync::Arc;

const USAGE: &str = "usage: forge-api <serve|migrate>";

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    let command = std::env::args().nth(1).unwrap_or_default();
    if !matches!(command.as_str(), "serve" | "migrate") {
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
    let app = router(AppState { pool, config: Arc::new(config) });
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(async {
            tokio::signal::ctrl_c().await.ok();
        })
        .await
        .map_err(|e| e.to_string())
}
