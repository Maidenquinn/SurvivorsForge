//! Backend API and worker of the SurvivorsForge platform.

pub mod app;
pub mod config;
pub mod error;
pub mod github;
pub mod jobs;
pub mod rate_limit;
pub mod worker;

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");
