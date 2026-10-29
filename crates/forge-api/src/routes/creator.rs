//! Small reads the creator pages of a client need: my addons, my rights on one addon, categories.

use super::addons::{load_addon, user_permissions};
use crate::app::AppState;
use crate::auth::CurrentUser;
use crate::error::ApiResult;
use axum::Json;
use axum::extract::{Path, State};
use serde_json::{Value, json};

/// Addons the user owns or helps with, with the trust of their newest version.
/// The user's addons: owned, shared, and `claimable` ones an admin added from a repo of the
/// user's account that nobody owns yet. The claim itself checks the repo rights on GitHub.
pub async fn my_addons(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Value>> {
    let list: Value = sqlx::query_scalar(
        "SELECT coalesce(json_agg(json_build_object(
                    'slug', a.slug, 'name', a.name,
                    'role', CASE WHEN a.owner_id = $1 THEN 'owner' WHEN a.owner_id IS NULL THEN 'claimable' ELSE 'contributor' END,
                    'blocked', a.blocked_at IS NOT NULL, 'locked', a.locked_at IS NOT NULL, 'latest_tag', v.tag, 'trust', t.trust,
                    'findings', json_build_object('critical', coalesce(t.critical, 0), 'warn', coalesce(t.warn, 0)))
                ORDER BY lower(a.name)), '[]')
         FROM addons a
         LEFT JOIN versions v ON v.id = relevant_version(a.id)
         LEFT JOIN version_trust t ON t.version_id = v.id
         WHERE a.deleted_at IS NULL
           AND (a.owner_id = $1 OR EXISTS (SELECT 1 FROM addon_members m WHERE m.addon_id = a.id AND m.user_id = $1)
                OR (a.owner_id IS NULL AND a.blocked_at IS NULL AND lower(split_part(a.repo_full_name, '/', 1)) = lower($2)))",
    )
    .bind(user.id)
    .bind(&user.login)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(list))
}

pub async fn my_permissions(State(state): State<AppState>, user: CurrentUser, Path(slug): Path<String>) -> ApiResult<Json<Value>> {
    let addon = load_addon(&state.pool, &slug).await?;
    let p = user_permissions(&state.pool, &addon, &user).await?;
    Ok(Json(json!({
        "owner": p.owner, "edit_description": p.edit_description, "edit_images": p.edit_images, "admin": p.admin,
        "blocked": addon.blocked_at.is_some(),
    })))
}

/// Every category in display order: each top-level one followed by its subcategories.
/// `selectable` is false for the ones with subcategories; addons go into the others.
pub async fn categories(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    let list: Value = sqlx::query_scalar(
        "SELECT coalesce(json_agg(json_build_object(
                    'name', c.name::text, 'parent', c.parent::text, 'sort_key', c.sort_key,
                    'selectable', NOT EXISTS (SELECT 1 FROM categories k WHERE k.parent = c.name))
                ORDER BY coalesce(p.sort_key, c.sort_key), coalesce(p.name, c.name), c.parent IS NOT NULL, c.sort_key, c.name), '[]')
         FROM categories c LEFT JOIN categories p ON p.name = c.parent",
    )
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(list))
}
