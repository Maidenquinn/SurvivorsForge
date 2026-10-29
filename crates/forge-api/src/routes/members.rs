//! `/v1/addons/{slug}/members`: contributors of an addon and what each may edit. Owner only.

use super::addons::{AddonRow, github_error, load_addon, not_blocked, require, user_permissions};
use crate::app::AppState;
use crate::auth::CurrentUser;
use crate::error::{ApiError, ApiResult};
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::Value;

async fn owned(state: &AppState, user: &CurrentUser, slug: &str) -> ApiResult<AddonRow> {
    let addon = load_addon(&state.pool, slug).await?;
    not_blocked(&addon)?;
    require(user_permissions(&state.pool, &addon, user).await?.owner, "Only the owner manages contributors")?;
    Ok(addon)
}

async fn members_json(state: &AppState, addon_id: i64) -> ApiResult<Value> {
    Ok(sqlx::query_scalar(
        "SELECT coalesce(json_agg(json_build_object(
                    'github_id', u.github_id, 'login', u.login, 'avatar_url', u.avatar_url,
                    'can_edit_description', m.can_edit_description, 'can_edit_images', m.can_edit_images)
                ORDER BY u.login), '[]')
         FROM addon_members m JOIN users u ON u.id = m.user_id WHERE m.addon_id = $1",
    )
    .bind(addon_id)
    .fetch_one(&state.pool)
    .await?)
}

pub async fn list(State(state): State<AppState>, user: CurrentUser, Path(slug): Path<String>) -> ApiResult<Json<Value>> {
    let addon = owned(&state, &user, &slug).await?;
    Ok(Json(members_json(&state, addon.id).await?))
}

#[derive(Deserialize)]
pub struct AddMember {
    login: String,
    #[serde(default)]
    can_edit_description: bool,
    #[serde(default)]
    can_edit_images: bool,
}

pub async fn add(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(slug): Path<String>,
    Json(body): Json<AddMember>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let addon = owned(&state, &user, &slug).await?;
    let login = body.login.trim().trim_start_matches('@');
    let valid = !login.is_empty() && login.len() <= 39 && login.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
    if !valid {
        return Err(ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, "invalid_login", "That is not a GitHub login"));
    }
    let github_user = state
        .github
        .user_by_login(addon.installation_id, login)
        .await
        .map_err(github_error)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "github_user_not_found", "No GitHub user with that login"))?;
    // A stub for someone who never logged in; their first login fills in the rest.
    let member_id: i64 = sqlx::query_scalar(
        "INSERT INTO users (github_id, login, avatar_url) VALUES ($1, $2, $3)
         ON CONFLICT (github_id) DO UPDATE SET login = EXCLUDED.login
         RETURNING id",
    )
    .bind(github_user.id)
    .bind(&github_user.login)
    .bind(&github_user.avatar_url)
    .fetch_one(&state.pool)
    .await?;
    if Some(member_id) == addon.owner_id {
        return Err(ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, "owner_not_member", "The owner can do everything already"));
    }
    let inserted = sqlx::query(
        "INSERT INTO addon_members (addon_id, user_id, can_edit_description, can_edit_images, added_by, added_at)
         VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT DO NOTHING",
    )
    .bind(addon.id)
    .bind(member_id)
    .bind(body.can_edit_description)
    .bind(body.can_edit_images)
    .bind(user.id)
    .bind((state.clock)())
    .execute(&state.pool)
    .await?;
    if inserted.rows_affected() == 0 {
        return Err(ApiError::new(StatusCode::CONFLICT, "member_exists", "This user is a contributor already"));
    }
    Ok((StatusCode::CREATED, Json(members_json(&state, addon.id).await?)))
}

#[derive(Deserialize)]
pub struct UpdateMember {
    can_edit_description: Option<bool>,
    can_edit_images: Option<bool>,
}

pub async fn update(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((slug, github_id)): Path<(String, i64)>,
    Json(body): Json<UpdateMember>,
) -> ApiResult<Json<Value>> {
    let addon = owned(&state, &user, &slug).await?;
    let changed = sqlx::query(
        "UPDATE addon_members m SET can_edit_description = coalesce($3, m.can_edit_description),
                                    can_edit_images = coalesce($4, m.can_edit_images)
         FROM users u WHERE u.id = m.user_id AND m.addon_id = $1 AND u.github_id = $2",
    )
    .bind(addon.id)
    .bind(github_id)
    .bind(body.can_edit_description)
    .bind(body.can_edit_images)
    .execute(&state.pool)
    .await?;
    if changed.rows_affected() == 0 {
        return Err(ApiError::not_found());
    }
    Ok(Json(members_json(&state, addon.id).await?))
}

pub async fn remove(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((slug, github_id)): Path<(String, i64)>,
) -> ApiResult<StatusCode> {
    let addon = owned(&state, &user, &slug).await?;
    let removed = sqlx::query("DELETE FROM addon_members m USING users u WHERE u.id = m.user_id AND m.addon_id = $1 AND u.github_id = $2")
        .bind(addon.id)
        .bind(github_id)
        .execute(&state.pool)
        .await?;
    if removed.rows_affected() == 0 {
        return Err(ApiError::not_found());
    }
    Ok(StatusCode::NO_CONTENT)
}
