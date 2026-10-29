//! Backend API and worker of the SurvivorsForge platform.

pub mod app;
pub mod config;
pub mod error;
pub mod rate_limit;

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");
