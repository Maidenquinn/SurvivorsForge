//! The append-only record of admin actions. Written in the same transaction as the action.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{Postgres, Transaction};

pub struct Entry<'a> {
    pub admin_id: i64,
    pub action: &'a str,
    pub target_type: &'a str,
    pub target_id: String,
    pub note: &'a str,
    pub details: Value,
}

pub async fn record(tx: &mut Transaction<'_, Postgres>, entry: Entry<'_>, now: DateTime<Utc>) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO audit_log (admin_id, action, target_type, target_id, note, details, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(entry.admin_id)
    .bind(entry.action)
    .bind(entry.target_type)
    .bind(entry.target_id)
    .bind(entry.note)
    .bind(entry.details)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
