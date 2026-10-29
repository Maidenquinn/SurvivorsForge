//! The job `check_sources`: is the scanned file still on GitHub, and is it still the same file?
//!
//! Release assets are compared by metadata first (asset ID, size, digest) and only downloaded when
//! that differs. Tag zips have no metadata, so they are downloaded and hashed each time.

use crate::app::AppState;
use crate::github::GitHubError;
use crate::scan_job::MAX_DOWNLOAD;
use crate::sync::SCAN_VERSION;
use crate::{audit, jobs};
use chrono::{DateTime, Utc};
use serde_json::json;
use sha2::{Digest, Sha256};

pub const CHECK_SOURCES: &str = "check_sources";

#[derive(sqlx::FromRow)]
struct Checked {
    id: i64,
    installation_id: i64,
    repo_id: i64,
    github_release_id: Option<i64>,
    asset_id: Option<i64>,
    asset_size: Option<i64>,
    github_digest: Option<String>,
    asset_url: String,
    sha256: String,
}

enum Outcome {
    Same,
    Missing,
    Changed,
}

async fn hash_changed(state: &AppState, v: &Checked) -> Result<Outcome, GitHubError> {
    match state.github.download(&v.asset_url, MAX_DOWNLOAD).await {
        Ok(bytes) if hex::encode(Sha256::digest(&bytes)) == v.sha256 => Ok(Outcome::Same),
        Ok(_) | Err(GitHubError::TooLarge) => Ok(Outcome::Changed),
        Err(GitHubError::NotFound) => Ok(Outcome::Missing),
        Err(e) => Err(e),
    }
}

async fn check_one(state: &AppState, v: &Checked) -> Result<Outcome, GitHubError> {
    let Some(release_id) = v.github_release_id else { return hash_changed(state, v).await };
    let Some(release) = state.github.release(v.installation_id, v.repo_id, release_id).await? else { return Ok(Outcome::Missing) };
    let Some(asset) = release.assets.iter().find(|a| Some(a.id) == v.asset_id) else { return Ok(Outcome::Missing) };
    let same_size = Some(asset.size) == v.asset_size;
    let same_digest = asset.digest == v.github_digest;
    if same_size && same_digest {
        return Ok(Outcome::Same);
    }
    hash_changed(state, v).await
}

/// Checks the `batch` versions checked longest ago.
pub async fn check_sources(state: &AppState, batch: i64) -> Result<(), String> {
    let versions: Vec<Checked> = sqlx::query_as(
        "SELECT v.id, a.installation_id, a.repo_id, v.github_release_id, v.asset_id, v.asset_size, v.github_digest, v.asset_url, v.sha256
         FROM versions v JOIN addons a ON a.id = v.addon_id
         WHERE v.sha256 IS NOT NULL AND a.deleted_at IS NULL AND a.locked_at IS NULL
         ORDER BY v.source_checked_at NULLS FIRST, v.id
         LIMIT $1",
    )
    .bind(batch)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| e.to_string())?;
    for v in versions {
        let now = (state.clock)();
        let outcome = match check_one(state, &v).await {
            Ok(outcome) => outcome,
            Err(GitHubError::RateLimited { .. }) => return Err("GitHub rate limit, the rest waits for the next run".into()),
            Err(e) => {
                tracing::warn!(version = v.id, error = %e, "source check failed");
                continue;
            }
        };
        mark(state, &v, outcome, now).await.map_err(|e| e.to_string())?;
    }
    Ok(())
}

async fn mark(state: &AppState, v: &Checked, outcome: Outcome, now: DateTime<Utc>) -> sqlx::Result<()> {
    let (status, changed) = match outcome {
        Outcome::Same => ("available", false),
        Outcome::Missing => ("missing", false),
        Outcome::Changed => ("available", true),
    };
    sqlx::query(
        "UPDATE versions SET source_status = $2, source_checked_at = $3,
                source_changed_at = CASE WHEN $4 THEN coalesce(source_changed_at, $3) ELSE source_changed_at END
         WHERE id = $1",
    )
    .bind(v.id)
    .bind(status)
    .bind(now)
    .bind(changed)
    .execute(&state.pool)
    .await?;
    if changed {
        jobs::enqueue(&state.pool, SCAN_VERSION, json!({ "version_id": v.id }), now, Some(&format!("scan:{}", v.id))).await?;
    }
    Ok(())
}

/// An admin accepts the new file: the version is judged by its new hash and scan from now on.
pub async fn accept_source(state: &AppState, admin_id: i64, version_id: i64, note: &str) -> sqlx::Result<bool> {
    let now = (state.clock)();
    let mut tx = state.pool.begin().await?;
    let accepted: Option<String> = sqlx::query_scalar(
        "UPDATE versions SET source_changed_at = NULL WHERE id = $1 AND source_changed_at IS NOT NULL RETURNING coalesce(sha256, '')",
    )
    .bind(version_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(sha256) = accepted else { return Ok(false) };
    let entry = audit::Entry {
        admin_id,
        action: "accept_source",
        target_type: "version",
        target_id: version_id.to_string(),
        note,
        details: json!({ "sha256": sha256 }),
    };
    audit::record(&mut tx, entry, now).await?;
    tx.commit().await?;
    Ok(true)
}
