//! Public addon discussions, distinct from issue-specific comments.
use crate::app::AppState;
use crate::auth::CurrentUser;
use crate::error::{ApiError, ApiResult};
use crate::routes::addons::{load_addon, user_permissions};
use axum::{Json, extract::{Path, Query, State}, http::StatusCode};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
pub struct Page { pub before: Option<i64> }
#[derive(Deserialize)]
pub struct Body { pub body: String }

fn invalid() -> ApiError {
    ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, "invalid_comment", "Comment must contain 1-10000 characters")
}
fn forbidden() -> ApiError {
    ApiError::new(StatusCode::FORBIDDEN, "forbidden", "You cannot modify this comment")
}
fn valid(text: &str) -> bool {
    let n = text.trim().chars().count();
    (1..=10000).contains(&n)
}

pub async fn list(State(state): State<AppState>, Path(slug): Path<String>, Query(page): Query<Page>)
    -> ApiResult<Json<Value>> {
    let addon = load_addon(&state.pool, &slug).await?;
    if addon.blocked_at.is_some() { return Err(ApiError::not_found()); }
    let before = page.before.unwrap_or(i64::MAX);
    let comments: Value = sqlx::query_scalar(
        "SELECT coalesce(json_agg(json_build_object(
            'id', c.id, 'body', c.body, 'author', u.login,
            'avatar_url', u.avatar_url, 'created_at', c.created_at, 'edited_at', c.edited_at
        ) ORDER BY c.created_at DESC, c.id DESC), '[]'::json)
        FROM (SELECT * FROM addon_comments
              WHERE addon_id=$1 AND id<$2 AND deleted_at IS NULL
              ORDER BY created_at DESC, id DESC LIMIT 30) c
        JOIN users u ON u.id=c.author_id"
    ).bind(addon.id).bind(before).fetch_one(&state.pool).await?;
    Ok(Json(comments))
}

pub async fn create(State(state): State<AppState>, user: CurrentUser,
    Path(slug): Path<String>, Json(body): Json<Body>)
    -> ApiResult<(StatusCode, Json<Value>)> {
    if !valid(&body.body) { return Err(invalid()); }
    let addon = load_addon(&state.pool, &slug).await?;
    if addon.blocked_at.is_some() { return Err(forbidden()); }
    let now = (state.clock)();
    let recent: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM addon_comments WHERE author_id=$1 AND created_at>$2"
    ).bind(user.id).bind(now - chrono::Duration::hours(24)).fetch_one(&state.pool).await?;
    if recent >= 100 {
        return Err(ApiError::new(StatusCode::TOO_MANY_REQUESTS, "comment_limit", "Daily comment limit reached"));
    }
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO addon_comments(addon_id,author_id,body,created_at) VALUES($1,$2,$3,$4) RETURNING id"
    ).bind(addon.id).bind(user.id).bind(body.body.trim()).bind(now).fetch_one(&state.pool).await?;
    Ok((StatusCode::CREATED, Json(json!({"id":id}))))
}

pub async fn edit(State(state): State<AppState>, user: CurrentUser,
    Path((slug,id)): Path<(String,i64)>, Json(body): Json<Body>) -> ApiResult<Json<Value>> {
    if !valid(&body.body) { return Err(invalid()); }
    let addon = load_addon(&state.pool, &slug).await?;
    if addon.blocked_at.is_some() { return Err(forbidden()); }
    let author: Option<i64> = sqlx::query_scalar(
        "SELECT author_id FROM addon_comments WHERE id=$1 AND addon_id=$2 AND deleted_at IS NULL"
    ).bind(id).bind(addon.id).fetch_optional(&state.pool).await?;
    let author = author.ok_or_else(ApiError::not_found)?;
    if author != user.id && !user.is_admin() { return Err(forbidden()); }
    sqlx::query("UPDATE addon_comments SET body=$3,edited_at=$4 WHERE id=$1 AND addon_id=$2 AND deleted_at IS NULL")
        .bind(id).bind(addon.id).bind(body.body.trim()).bind((state.clock)()).execute(&state.pool).await?;
    Ok(Json(json!({"id":id,"edited":true})))
}

pub async fn remove(State(state): State<AppState>, user: CurrentUser,
    Path((slug,id)): Path<(String,i64)>) -> ApiResult<Json<Value>> {
    let addon = load_addon(&state.pool, &slug).await?;
    let author: Option<i64> = sqlx::query_scalar(
        "SELECT author_id FROM addon_comments WHERE id=$1 AND addon_id=$2 AND deleted_at IS NULL"
    ).bind(id).bind(addon.id).fetch_optional(&state.pool).await?;
    let author = author.ok_or_else(ApiError::not_found)?;
    let rights = user_permissions(&state.pool, &addon, &user).await?;
    if author != user.id && !user.is_admin() && !rights.owner && !rights.edit_description {
        return Err(forbidden());
    }
    sqlx::query("UPDATE addon_comments SET deleted_at=$3,deleted_by=$4 WHERE id=$1 AND addon_id=$2 AND deleted_at IS NULL")
        .bind(id).bind(addon.id).bind((state.clock)()).bind(user.id).execute(&state.pool).await?;
    Ok(Json(json!({"id":id,"deleted":true})))
}
