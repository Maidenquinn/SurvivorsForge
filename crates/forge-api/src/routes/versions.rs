//! `/v1/addons/{slug}/versions`: the version history, newest first, including versions gone from GitHub.

use super::addons::load_addon;
use crate::app::AppState;
use crate::error::ApiResult;
use axum::Json;
use crate::error::ApiError;
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

/// The latest scan of a version with all its findings.
pub async fn findings(State(state): State<AppState>, Path((slug, tag)): Path<(String, String)>) -> ApiResult<Json<Value>> {
    let addon = load_addon(&state.pool, &slug).await?;
    let report: Option<Value> = sqlx::query_scalar(
        "SELECT json_build_object(
             'tag', v.tag, 'scan_status', v.scan_status,
             'scan', (SELECT json_build_object(
                          'result', s.result, 'scanner_version', s.scanner_version, 'sha256', s.sha256,
                          'finished_at', s.finished_at,
                          'findings', (SELECT coalesce(json_agg(json_build_object(
                                              'rule', f.rule, 'severity', f.severity, 'file', f.file, 'line', f.line,
                                              'message', f.message, 'snippet', f.snippet)
                                          ORDER BY CASE f.severity WHEN 'critical' THEN 0 WHEN 'warn' THEN 1 ELSE 2 END, f.id), '[]')
                                       FROM findings f WHERE f.scan_id = s.id))
                      FROM scans s WHERE s.version_id = v.id ORDER BY s.id DESC LIMIT 1))
         FROM versions v WHERE v.addon_id = $1 AND v.tag = $2",
    )
    .bind(addon.id)
    .bind(&tag)
    .fetch_optional(&state.pool)
    .await?;
    report.map(Json).ok_or_else(ApiError::not_found)
}
