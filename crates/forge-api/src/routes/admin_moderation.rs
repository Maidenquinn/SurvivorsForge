//! Admin blocks of addons and versions, and bans of users. Every action needs a note and is audited.

use super::reports::note_required;
use crate::app::AppState;
use crate::audit::{self, Entry};
use crate::auth::AdminUser;
use crate::error::{ApiError, ApiResult};
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize, Default)]
pub struct Note {
    #[serde(default)]
    note: String,
}

fn note_of(body: Option<Json<Note>>) -> String {
    body.map(|b| b.0.note).unwrap_or_default()
}

async fn finish(state: &AppState, mut tx: sqlx::Transaction<'_, sqlx::Postgres>, entry: Entry<'_>) -> ApiResult<StatusCode> {
    audit::record(&mut tx, entry, (state.clock)()).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn block_addon(State(state): State<AppState>, AdminUser(admin): AdminUser, Path(slug): Path<String>, body: Option<Json<Note>>) -> ApiResult<StatusCode> {
    let note = note_of(body);
    let note = note_required(&note)?;
    let mut tx = state.pool.begin().await?;
    let id: Option<i64> = sqlx::query_scalar(
        "UPDATE addons SET blocked_at = $2, block_reason = $3, blocked_by_ban = false WHERE slug = $1 AND deleted_at IS NULL RETURNING id",
    )
    .bind(&slug)
    .bind((state.clock)())
    .bind(note)
    .fetch_optional(&mut *tx)
    .await?;
    let id = id.ok_or_else(ApiError::not_found)?;
    finish(&state, tx, Entry { admin_id: admin.id, action: "block_addon", target_type: "addon", target_id: id.to_string(), note, details: json!({ "slug": slug }) }).await
}

pub async fn unblock_addon(State(state): State<AppState>, AdminUser(admin): AdminUser, Path(slug): Path<String>, body: Option<Json<Note>>) -> ApiResult<StatusCode> {
    let note = note_of(body);
    let note = note_required(&note)?;
    let mut tx = state.pool.begin().await?;
    let id: Option<i64> = sqlx::query_scalar(
        "UPDATE addons SET blocked_at = NULL, block_reason = NULL, blocked_by_ban = false
         WHERE slug = $1 AND deleted_at IS NULL AND blocked_at IS NOT NULL RETURNING id",
    )
    .bind(&slug)
    .fetch_optional(&mut *tx)
    .await?;
    let id = id.ok_or_else(ApiError::not_found)?;
    finish(&state, tx, Entry { admin_id: admin.id, action: "unblock_addon", target_type: "addon", target_id: id.to_string(), note, details: json!({ "slug": slug }) }).await
}

pub async fn block_version(State(state): State<AppState>, AdminUser(admin): AdminUser, Path(id): Path<i64>, body: Option<Json<Note>>) -> ApiResult<StatusCode> {
    let note = note_of(body);
    let note = note_required(&note)?;
    let mut tx = state.pool.begin().await?;
    let changed = sqlx::query("UPDATE versions SET blocked_at = $2, block_reason = $3 WHERE id = $1")
        .bind(id)
        .bind((state.clock)())
        .bind(note)
        .execute(&mut *tx)
        .await?;
    if changed.rows_affected() == 0 {
        return Err(ApiError::not_found());
    }
    finish(&state, tx, Entry { admin_id: admin.id, action: "block_version", target_type: "version", target_id: id.to_string(), note, details: json!({}) }).await
}

pub async fn unblock_version(State(state): State<AppState>, AdminUser(admin): AdminUser, Path(id): Path<i64>, body: Option<Json<Note>>) -> ApiResult<StatusCode> {
    let note = note_of(body);
    let note = note_required(&note)?;
    let mut tx = state.pool.begin().await?;
    let changed = sqlx::query("UPDATE versions SET blocked_at = NULL, block_reason = NULL WHERE id = $1 AND blocked_at IS NOT NULL")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    if changed.rows_affected() == 0 {
        return Err(ApiError::not_found());
    }
    finish(&state, tx, Entry { admin_id: admin.id, action: "unblock_version", target_type: "version", target_id: id.to_string(), note, details: json!({}) }).await
}

/// Bans a user: no login, no access, and their own addons are blocked until the ban is lifted.
pub async fn ban(State(state): State<AppState>, AdminUser(admin): AdminUser, Path(github_id): Path<i64>, body: Option<Json<Note>>) -> ApiResult<StatusCode> {
    let note = note_of(body);
    let note = note_required(&note)?;
    let target: Option<(i64, String)> = sqlx::query_as("SELECT id, role FROM users WHERE github_id = $1").bind(github_id).fetch_optional(&state.pool).await?;
    let (user_id, role) = target.ok_or_else(ApiError::not_found)?;
    if role == "admin" || state.config.admin_github_ids.contains(&github_id) {
        return Err(ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, "cannot_ban_admin", "Admins cannot be banned"));
    }
    let now = (state.clock)();
    let mut tx = state.pool.begin().await?;
    sqlx::query("UPDATE users SET banned_at = $2, ban_reason = $3 WHERE id = $1").bind(user_id).bind(now).bind(note).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM sessions WHERE user_id = $1").bind(user_id).execute(&mut *tx).await?;
    let blocked = sqlx::query(
        "UPDATE addons SET blocked_at = $2, block_reason = 'owner_banned', blocked_by_ban = true
         WHERE owner_id = $1 AND deleted_at IS NULL AND blocked_at IS NULL",
    )
    .bind(user_id)
    .bind(now)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    let details = json!({ "blocked_addons": blocked });
    finish(&state, tx, Entry { admin_id: admin.id, action: "ban_user", target_type: "user", target_id: github_id.to_string(), note, details }).await
}

pub async fn unban(State(state): State<AppState>, AdminUser(admin): AdminUser, Path(github_id): Path<i64>, body: Option<Json<Note>>) -> ApiResult<StatusCode> {
    let note = note_of(body);
    let note = note_required(&note)?;
    let mut tx = state.pool.begin().await?;
    let user_id: Option<i64> = sqlx::query_scalar(
        "UPDATE users SET banned_at = NULL, ban_reason = NULL WHERE github_id = $1 AND banned_at IS NOT NULL RETURNING id",
    )
    .bind(github_id)
    .fetch_optional(&mut *tx)
    .await?;
    let user_id = user_id.ok_or_else(ApiError::not_found)?;
    sqlx::query("UPDATE addons SET blocked_at = NULL, block_reason = NULL, blocked_by_ban = false WHERE owner_id = $1 AND blocked_by_ban")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    finish(&state, tx, Entry { admin_id: admin.id, action: "unban_user", target_type: "user", target_id: github_id.to_string(), note, details: json!({}) }).await
}
