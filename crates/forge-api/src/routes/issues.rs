//! Internal issue tracker for addon bugs, requests and questions.
use crate::app::AppState;
use crate::auth::CurrentUser;
use crate::error::{ApiError, ApiResult};
use crate::routes::addons::{load_addon, user_permissions};
use axum::{Json, extract::{Path, State}, http::StatusCode};
use serde::Deserialize;
use serde_json::{json, Value};

fn invalid(message: &str) -> ApiError {
    ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, "invalid_issue", message)
}
fn forbidden() -> ApiError {
    ApiError::new(StatusCode::FORBIDDEN, "forbidden", "Only addon maintainers can change issue status")
}
fn valid_text(text: &str, min: usize, max: usize) -> bool {
    let n = text.trim().chars().count();
    n >= min && n <= max
}
#[derive(Deserialize)]
pub struct NewIssue { title: String, body: String, #[serde(default = "default_kind")] kind: String }
fn default_kind() -> String { "bug".to_owned() }
#[derive(Deserialize)]
pub struct NewComment { body: String }
#[derive(Deserialize)]
pub struct ChangeStatus { status: String }

pub async fn list(State(state): State<AppState>, Path(slug): Path<String>) -> ApiResult<Json<Value>> {
    let addon = load_addon(&state.pool, &slug).await?;
    let items: Value = sqlx::query_scalar(
        "SELECT coalesce(json_agg(json_build_object('id', i.id, 'title', i.title, 'kind', i.kind,
          'status', i.status, 'author', u.login, 'created_at', i.created_at, 'updated_at', i.updated_at)
          ORDER BY i.created_at DESC, i.id DESC), '[]'::json)
         FROM (SELECT * FROM addon_issues WHERE addon_id = $1 ORDER BY created_at DESC, id DESC LIMIT 100) i
         JOIN users u ON u.id = i.author_id"
    ).bind(addon.id).fetch_one(&state.pool).await?;
    Ok(Json(items))
}
pub async fn create(State(state): State<AppState>, user: CurrentUser, Path(slug): Path<String>, Json(body): Json<NewIssue>)
    -> ApiResult<(StatusCode, Json<Value>)> {
    if !valid_text(&body.title, 3, 160) || !valid_text(&body.body, 1, 10000) ||
        !matches!(body.kind.as_str(), "bug" | "feature" | "question") {
        return Err(invalid("Title must be 3-160 characters, body 1-10000, kind bug/feature/question"));
    }
    let addon = load_addon(&state.pool, &slug).await?;
    if addon.blocked_at.is_some() { return Err(forbidden()); }
    let now = (state.clock)();
    let recent: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM addon_issues WHERE author_id = $1 AND created_at > $2"
    ).bind(user.id).bind(now - chrono::Duration::hours(24)).fetch_one(&state.pool).await?;
    if recent >= 20 { return Err(ApiError::new(StatusCode::TOO_MANY_REQUESTS, "issue_limit", "Daily issue limit reached")); }
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO addon_issues (addon_id, author_id, title, body, kind, created_at, updated_at)
         VALUES ($1,$2,$3,$4,$5,$6,$6) RETURNING id"
    ).bind(addon.id).bind(user.id).bind(body.title.trim()).bind(body.body.trim())
     .bind(&body.kind).bind(now).fetch_one(&state.pool).await?;
    Ok((StatusCode::CREATED, Json(json!({"id":id}))))
}
pub async fn show(State(state): State<AppState>, Path((slug, id)): Path<(String, i64)>) -> ApiResult<Json<Value>> {
    let addon = load_addon(&state.pool, &slug).await?;
    let issue: Option<Value> = sqlx::query_scalar(
        "SELECT json_build_object('id', i.id, 'title', i.title, 'body', i.body, 'kind', i.kind,
         'status', i.status, 'author', u.login, 'created_at', i.created_at, 'updated_at', i.updated_at,
         'comments', (SELECT coalesce(json_agg(json_build_object('id', c.id, 'body', c.body,
         'author', cu.login, 'created_at', c.created_at) ORDER BY c.created_at, c.id), '[]'::json)
         FROM addon_issue_comments c JOIN users cu ON cu.id = c.author_id WHERE c.issue_id = i.id))
         FROM addon_issues i JOIN users u ON u.id = i.author_id WHERE i.id = $1 AND i.addon_id = $2"
    ).bind(id).bind(addon.id).fetch_optional(&state.pool).await?;
    Ok(Json(issue.ok_or_else(ApiError::not_found)?))
}
pub async fn comment(State(state): State<AppState>, user: CurrentUser, Path((slug, id)): Path<(String, i64)>,
    Json(body): Json<NewComment>) -> ApiResult<(StatusCode, Json<Value>)> {
    if !valid_text(&body.body, 1, 10000) { return Err(invalid("Comment must be 1-10000 characters")); }
    let addon = load_addon(&state.pool, &slug).await?;
    if addon.blocked_at.is_some() { return Err(forbidden()); }
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM addon_issues WHERE id=$1 AND addon_id=$2)"
    ).bind(id).bind(addon.id).fetch_one(&state.pool).await?;
    if !exists { return Err(ApiError::not_found()); }
    let now = (state.clock)();
    let recent: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM addon_issue_comments WHERE author_id=$1 AND created_at>$2"
    ).bind(user.id).bind(now - chrono::Duration::hours(24)).fetch_one(&state.pool).await?;
    if recent >= 100 { return Err(ApiError::new(StatusCode::TOO_MANY_REQUESTS, "comment_limit", "Daily comment limit reached")); }
    let comment_id: i64 = sqlx::query_scalar(
        "INSERT INTO addon_issue_comments(issue_id, author_id, body, created_at) VALUES($1,$2,$3,$4) RETURNING id"
    ).bind(id).bind(user.id).bind(body.body.trim()).bind(now).fetch_one(&state.pool).await?;
    Ok((StatusCode::CREATED, Json(json!({"id":comment_id}))))
}
pub async fn set_status(State(state): State<AppState>, user: CurrentUser, Path((slug,id)): Path<(String,i64)>,
    Json(body): Json<ChangeStatus>) -> ApiResult<Json<Value>> {
    if !matches!(body.status.as_str(), "open" | "acknowledged" | "in_progress" | "resolved" | "closed") {
        return Err(invalid("Unknown issue status"));
    }
    let addon = load_addon(&state.pool, &slug).await?;
    let rights = user_permissions(&state.pool, &addon, &user).await?;
    if !rights.owner && !rights.admin && !rights.edit_description { return Err(forbidden()); }
    if addon.blocked_at.is_some() { return Err(forbidden()); }
    let changed = sqlx::query(
        "UPDATE addon_issues SET status=$3, updated_at=$4 WHERE id=$1 AND addon_id=$2"
    ).bind(id).bind(addon.id).bind(&body.status).bind((state.clock)())
     .execute(&state.pool).await?;
    if changed.rows_affected()==0 { return Err(ApiError::not_found()); }
    Ok(Json(json!({"id":id,"status":body.status})))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn text_bounds() {
        assert!(!valid_text("  ",1,100));
        assert!(valid_text("a",1,100));
        assert!(!valid_text("ab",3,160));
        assert!(!valid_text(&"a".repeat(10001),1,10000));
    }
}
