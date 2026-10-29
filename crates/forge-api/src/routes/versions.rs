//! `/v1/addons/{slug}/versions`: the version history, newest first, including versions gone from GitHub.

use super::addons::load_addon;
use crate::app::AppState;
use crate::error::ApiResult;
use axum::Json;
use axum::extract::{Path, State};
use serde_json::Value;

pub async fn list(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> ApiResult<Json<Value>> {
    let addon = load_addon(&state.pool, &slug).await?;
    let versions: Value = sqlx::query_scalar(
        "SELECT coalesce(json_agg(json_build_object(
                    'tag', tag, 'name', name, 'prerelease', prerelease, 'published_at', published_at,
                    'asset_name', asset_name, 'asset_url', asset_url, 'size', asset_size, 'sha256', sha256,
                    'downloads', download_count, 'scan_status', scan_status, 'source', source_status,
                    'deleted_on_github', deleted_on_github_at IS NOT NULL)
                ORDER BY published_at DESC, id DESC), '[]')
         FROM versions WHERE addon_id = $1",
    )
    .bind(addon.id)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(versions))
}
