//! Admin reads and the category list: the review queue, the audit log and user lookup.

use crate::app::AppState;
use crate::audit::{self, Entry};
use crate::auth::AdminUser;
use crate::error::{ApiError, ApiResult};
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::{Value, json};

/// Versions an admin should look at, oldest first: flagged ones, changed files, and new versions
/// of addons whose author already had a verified version.
pub async fn review_queue(State(state): State<AppState>, AdminUser(_): AdminUser) -> ApiResult<Json<Value>> {
    let list: Value = sqlx::query_scalar(
        "WITH queue AS (
             SELECT v.id, v.tag, v.created_at, v.addon_id, t.trust,
                    (SELECT p.tag FROM versions p WHERE p.addon_id = v.addon_id AND p.published_at < v.published_at
                     ORDER BY p.published_at DESC LIMIT 1) AS previous_tag
             FROM versions v
             JOIN version_trust t ON t.version_id = v.id
             JOIN addons a ON a.id = v.addon_id
             WHERE a.deleted_at IS NULL AND v.deleted_on_github_at IS NULL
               AND (t.trust IN ('flagged', 'source_changed')
                    OR (t.trust = 'scanned' AND EXISTS (
                            SELECT 1 FROM verifications w JOIN versions wv ON wv.id = w.version_id WHERE wv.addon_id = a.id))))
         SELECT coalesce(json_agg(json_build_object(
                    'version_id', q.id, 'tag', q.tag, 'trust', q.trust, 'slug', a.slug, 'name', a.name, 'repo', a.repo_full_name,
                    'compare_url', CASE WHEN q.previous_tag IS NULL THEN NULL
                                        ELSE 'https://github.com/' || a.repo_full_name || '/compare/' || q.previous_tag || '...' || q.tag END,
                    'findings', (SELECT coalesce(json_agg(json_build_object(
                                        'rule', f.rule, 'severity', f.severity, 'file', f.file, 'line', f.line, 'message', f.message, 'snippet', f.snippet)
                                        ORDER BY f.severity, f.id), '[]')
                                 FROM findings f WHERE f.scan_id = (SELECT s.id FROM scans s WHERE s.version_id = q.id ORDER BY s.id DESC LIMIT 1)
                                   AND f.severity <> 'info'))
                ORDER BY q.created_at, q.id), '[]')
         FROM queue q JOIN addons a ON a.id = q.addon_id",
    )
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(list))
}

#[derive(Deserialize)]
pub struct AuditQuery {
    admin: Option<String>,
    action: Option<String>,
    target_type: Option<String>,
    target_id: Option<String>,
    page: Option<i64>,
}

pub const AUDIT_PAGE: i64 = 50;

pub async fn audit_log(State(state): State<AppState>, AdminUser(_): AdminUser, Query(q): Query<AuditQuery>) -> ApiResult<Json<Value>> {
    let page = q.page.unwrap_or(1).max(1);
    let empty = |v: &Option<String>| v.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    let page_json: Value = sqlx::query_scalar(
        "WITH hits AS (
             SELECT l.*, u.login::text AS admin_login FROM audit_log l JOIN users u ON u.id = l.admin_id
             WHERE ($1::text IS NULL OR u.login = $1::citext) AND ($2::text IS NULL OR l.action = $2)
               AND ($3::text IS NULL OR l.target_type = $3) AND ($4::text IS NULL OR l.target_id = $4))
         SELECT json_build_object(
             'total', (SELECT count(*) FROM hits), 'page', $5::bigint, 'per_page', $6::bigint,
             'entries', coalesce((SELECT json_agg(json_build_object(
                             'id', h.id, 'admin', h.admin_login, 'action', h.action, 'target_type', h.target_type,
                             'target_id', h.target_id, 'note', h.note, 'details', h.details, 'created_at', h.created_at)
                             ORDER BY h.id DESC)
                          FROM (SELECT * FROM hits ORDER BY id DESC LIMIT $6 OFFSET ($5 - 1) * $6) h), '[]'))",
    )
    .bind(empty(&q.admin))
    .bind(empty(&q.action))
    .bind(empty(&q.target_type))
    .bind(empty(&q.target_id))
    .bind(page)
    .bind(AUDIT_PAGE)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(page_json))
}

/// A platform user by GitHub login, for the ban and owner forms.
pub async fn user(State(state): State<AppState>, AdminUser(_): AdminUser, Path(login): Path<String>) -> ApiResult<Json<Value>> {
    let user: Option<Value> = sqlx::query_scalar(
        "SELECT json_build_object('github_id', u.github_id, 'login', u.login, 'role', u.role, 'banned', u.banned_at IS NOT NULL,
                                  'ban_reason', u.ban_reason, 'addons', (SELECT count(*) FROM addons WHERE owner_id = u.id AND deleted_at IS NULL))
         FROM users u WHERE u.login = $1::citext",
    )
    .bind(login.trim())
    .fetch_optional(&state.pool)
    .await?;
    user.map(Json).ok_or_else(ApiError::not_found)
}

#[derive(Deserialize)]
pub struct CategoryBody {
    name: Option<String>,
    sort_key: Option<i32>,
    #[serde(default)]
    note: String,
}

fn valid_category(name: &str) -> ApiResult<&str> {
    let name = name.trim();
    if name.is_empty() || name.len() > 32 || name.chars().any(char::is_control) {
        return Err(ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, "invalid_name", "A category name is 1 to 32 characters"));
    }
    Ok(name)
}

pub async fn create_category(State(state): State<AppState>, AdminUser(admin): AdminUser, Json(body): Json<CategoryBody>) -> ApiResult<StatusCode> {
    let name = valid_category(body.name.as_deref().unwrap_or_default())?;
    let mut tx = state.pool.begin().await?;
    let inserted = sqlx::query("INSERT INTO categories (name, sort_key) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(name)
        .bind(body.sort_key.unwrap_or(100))
        .execute(&mut *tx)
        .await?;
    if inserted.rows_affected() == 0 {
        return Err(ApiError::new(StatusCode::CONFLICT, "name_taken", "This category exists already"));
    }
    let entry = Entry { admin_id: admin.id, action: "category_create", target_type: "category", target_id: name.into(), note: &body.note, details: json!({}) };
    audit::record(&mut tx, entry, (state.clock)()).await?;
    tx.commit().await?;
    Ok(StatusCode::CREATED)
}

/// Renames or reorders; addons follow a rename through the foreign key.
pub async fn update_category(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Path(name): Path<String>,
    Json(body): Json<CategoryBody>,
) -> ApiResult<StatusCode> {
    let new_name = match &body.name {
        Some(n) => Some(valid_category(n)?.to_string()),
        None => None,
    };
    let mut tx = state.pool.begin().await?;
    let changed = sqlx::query("UPDATE categories SET name = coalesce($2, name), sort_key = coalesce($3, sort_key) WHERE name = $1::citext")
        .bind(&name)
        .bind(&new_name)
        .bind(body.sort_key)
        .execute(&mut *tx)
        .await
        .map_err(|e| match &e {
            sqlx::Error::Database(db) if db.is_unique_violation() => ApiError::new(StatusCode::CONFLICT, "name_taken", "This category exists already"),
            _ => e.into(),
        })?;
    if changed.rows_affected() == 0 {
        return Err(ApiError::not_found());
    }
    let details = json!({ "name": new_name, "sort_key": body.sort_key });
    let entry = Entry { admin_id: admin.id, action: "category_update", target_type: "category", target_id: name, note: &body.note, details };
    audit::record(&mut tx, entry, (state.clock)()).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_category(State(state): State<AppState>, AdminUser(admin): AdminUser, Path(name): Path<String>) -> ApiResult<StatusCode> {
    let used: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM addons WHERE category = $1::citext)").bind(&name).fetch_one(&state.pool).await?;
    if used {
        return Err(ApiError::new(StatusCode::CONFLICT, "category_in_use", "Addons still use this category"));
    }
    let mut tx = state.pool.begin().await?;
    let removed = sqlx::query("DELETE FROM categories WHERE name = $1::citext").bind(&name).execute(&mut *tx).await?;
    if removed.rows_affected() == 0 {
        return Err(ApiError::not_found());
    }
    let entry = Entry { admin_id: admin.id, action: "category_delete", target_type: "category", target_id: name, note: "", details: json!({}) };
    audit::record(&mut tx, entry, (state.clock)()).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
