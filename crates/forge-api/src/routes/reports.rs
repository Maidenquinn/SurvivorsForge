//! `/v1/reports`: logged-in users report addons, versions and images; admins work the queue.

use crate::app::AppState;
use crate::audit::{self, Entry};
use crate::auth::{AdminUser, CurrentUser};
use crate::error::{ApiError, ApiResult};
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::Duration;
use serde::Deserialize;
use serde_json::{Value, json};

pub const REASONS: &[&str] = &["malware", "stolen", "broken", "inappropriate", "spam", "other"];
pub const MAX_PER_DAY: i64 = 10;
pub const MAX_TEXT: usize = 2000;

fn invalid(code: &'static str, message: &str) -> ApiError {
    ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, code, message)
}

/// A non-empty note for an admin action, or `422 note_required`.
pub fn note_required(note: &str) -> ApiResult<&str> {
    let note = note.trim();
    if note.is_empty() { Err(invalid("note_required", "Admin actions need a note")) } else { Ok(note) }
}

#[derive(Deserialize)]
pub struct NewReport {
    target_type: String,
    /// The slug for an addon, the numeric id for a version or an image.
    target: String,
    reason: String,
    #[serde(default)]
    text: String,
}

pub async fn create(State(state): State<AppState>, user: CurrentUser, Json(body): Json<NewReport>) -> ApiResult<(StatusCode, Json<Value>)> {
    if !REASONS.contains(&body.reason.as_str()) {
        return Err(invalid("invalid_reason", "Unknown reason"));
    }
    if body.text.chars().count() > MAX_TEXT {
        return Err(invalid("text_too_long", "At most 2000 characters"));
    }
    let target_id: Option<i64> = match body.target_type.as_str() {
        "addon" => sqlx::query_scalar("SELECT id FROM addons WHERE slug = $1::citext AND deleted_at IS NULL").bind(&body.target).fetch_optional(&state.pool).await?,
        "version" | "image" => {
            let id: i64 = body.target.parse().map_err(|_| ApiError::not_found())?;
            let table = if body.target_type == "version" { "versions" } else { "images" };
            sqlx::query_scalar(&format!("SELECT id FROM {table} WHERE id = $1")).bind(id).fetch_optional(&state.pool).await?
        }
        _ => return Err(invalid("invalid_target", "target_type is addon, version or image")),
    };
    let target_id = target_id.ok_or_else(ApiError::not_found)?;
    let now = (state.clock)();
    let today: i64 = sqlx::query_scalar("SELECT count(*) FROM reports WHERE reporter_id = $1 AND created_at > $2")
        .bind(user.id)
        .bind(now - Duration::days(1))
        .fetch_one(&state.pool)
        .await?;
    if today >= MAX_PER_DAY {
        return Err(ApiError::new(StatusCode::TOO_MANY_REQUESTS, "report_limit", "At most 10 reports a day"));
    }
    let id: Option<i64> = sqlx::query_scalar(
        "INSERT INTO reports (reporter_id, target_type, target_id, reason, text, created_at) VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (reporter_id, target_type, target_id) WHERE status = 'open' DO NOTHING RETURNING id",
    )
    .bind(user.id)
    .bind(&body.target_type)
    .bind(target_id)
    .bind(&body.reason)
    .bind(body.text.trim())
    .bind(now)
    .fetch_optional(&state.pool)
    .await?;
    let id = id.ok_or_else(|| ApiError::new(StatusCode::CONFLICT, "already_reported", "You reported this already"))?;
    Ok((StatusCode::CREATED, Json(json!({ "id": id }))))
}

/// Open reports, oldest first.
pub async fn queue(State(state): State<AppState>, AdminUser(_): AdminUser) -> ApiResult<Json<Value>> {
    let list: Value = sqlx::query_scalar(
        "SELECT coalesce(json_agg(json_build_object(
                    'id', r.id, 'target_type', r.target_type, 'target_id', r.target_id,
                    'addon', CASE r.target_type
                                 WHEN 'addon' THEN (SELECT slug FROM addons WHERE id = r.target_id)
                                 WHEN 'version' THEN (SELECT a.slug FROM versions v JOIN addons a ON a.id = v.addon_id WHERE v.id = r.target_id)
                                 ELSE (SELECT a.slug FROM images i JOIN addons a ON a.id = i.addon_id WHERE i.id = r.target_id) END,
                    'reason', r.reason, 'text', r.text, 'reporter', u.login, 'created_at', r.created_at)
                ORDER BY r.created_at, r.id), '[]')
         FROM reports r JOIN users u ON u.id = r.reporter_id WHERE r.status = 'open'",
    )
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(list))
}

#[derive(Deserialize)]
pub struct Resolve {
    status: String,
    #[serde(default)]
    note: String,
}

pub async fn resolve(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Path(id): Path<i64>,
    Json(body): Json<Resolve>,
) -> ApiResult<StatusCode> {
    if !matches!(body.status.as_str(), "dismissed" | "actioned") {
        return Err(invalid("invalid_status", "status is dismissed or actioned"));
    }
    let note = note_required(&body.note)?;
    let now = (state.clock)();
    let mut tx = state.pool.begin().await?;
    let changed = sqlx::query("UPDATE reports SET status = $2, handled_by = $3, handled_at = $4 WHERE id = $1 AND status = 'open'")
        .bind(id)
        .bind(&body.status)
        .bind(admin.id)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    if changed.rows_affected() == 0 {
        return Err(ApiError::not_found());
    }
    let entry = Entry { admin_id: admin.id, action: "resolve_report", target_type: "report", target_id: id.to_string(), note, details: json!({ "status": body.status }) };
    audit::record(&mut tx, entry, now).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}


#[derive(Deserialize)]
pub struct CommentResolution {
    status: String,
    #[serde(default)]
    note: String,
}

/// Private queue for comment reports; never expose reporter identities publicly.
pub async fn comment_queue(State(state): State<AppState>, AdminUser(_): AdminUser) -> ApiResult<Json<Value>> {
    let rows: Value = sqlx::query_scalar(
        "SELECT coalesce(json_agg(json_build_object(
          'id', r.id, 'comment_id', r.comment_id, 'addon', a.slug,
          'author', author.login, 'comment', c.body, 'reason', r.reason,
          'details', r.details, 'reporter', reporter.login, 'created_at', r.created_at
        ) ORDER BY r.created_at, r.id), '[]'::json)
        FROM addon_comment_reports r
        JOIN addon_comments c ON c.id=r.comment_id
        JOIN addons a ON a.id=c.addon_id
        JOIN users reporter ON reporter.id=r.reporter_id
        JOIN users author ON author.id=c.author_id
        WHERE r.resolved_at IS NULL"
    ).fetch_one(&state.pool).await?;
    Ok(Json(rows))
}

/// Resolve a comment report; optionally remove the reported comment in the same transaction.
pub async fn resolve_comment(
    State(state): State<AppState>, AdminUser(admin): AdminUser,
    Path(id): Path<i64>, Json(body): Json<CommentResolution>
) -> ApiResult<StatusCode> {
    if !matches!(body.status.as_str(), "dismissed" | "actioned") {
        return Err(invalid("invalid_status", "status must be dismissed or actioned"));
    }
    let note = note_required(&body.note)?;
    let now = (state.clock)();
    let mut tx = state.pool.begin().await?;
    let target: Option<i64> = sqlx::query_scalar(
        "UPDATE addon_comment_reports SET resolved_at=$2, resolved_by=$3, resolution_note=$4
         WHERE id=$1 AND resolved_at IS NULL RETURNING comment_id"
    ).bind(id).bind(now).bind(admin.id).bind(note).fetch_optional(&mut *tx).await?;
    let target = target.ok_or_else(ApiError::not_found)?;
    if body.status == "actioned" {
        sqlx::query("UPDATE addon_comments SET deleted_at=coalesce(deleted_at,$2), deleted_by=coalesce(deleted_by,$3) WHERE id=$1")
            .bind(target).bind(now).bind(admin.id).execute(&mut *tx).await?;
    }
    audit::record(&mut tx, Entry {
        admin_id: admin.id, action: "resolve_comment_report", target_type: "comment_report",
        target_id: id.to_string(), note,
        details: json!({"status": body.status, "comment_id": target}),
    }, now).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
