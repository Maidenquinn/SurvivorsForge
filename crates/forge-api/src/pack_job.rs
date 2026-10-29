//! Worker jobs of addon packs: build a pack's zip from the checked versions, and delete zips that
//! nobody downloaded for a while or that do not fit the storage budget.

use crate::app::AppState;
use crate::scan_job::MAX_DOWNLOAD;
use chrono::Duration;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::Cursor;
use std::path::PathBuf;

pub const KIND: &str = "build_pack";
pub const PRUNE: &str = "prune_packs";

#[derive(sqlx::FromRow)]
struct Part {
    asset_url: String,
    sha256: Option<String>,
    name: String,
    tag: String,
}

/// The zip of `key`; keys are SHA-256 hex, so nothing else can end up in the path.
pub fn zip_path(state: &AppState, key: &str) -> Option<PathBuf> {
    (key.len() == 64 && key.bytes().all(|b| b.is_ascii_hexdigit())).then(|| PathBuf::from(&state.config.pack_dir).join(format!("{key}.zip")))
}

async fn fail(state: &AppState, key: &str, message: &str) -> Result<(), String> {
    sqlx::query("UPDATE pack_files SET status = 'failed', error = $2 WHERE content_key = $1")
        .bind(key)
        .bind(message)
        .execute(&state.pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Builds the zip of `payload.content_key`. Problems with the pack itself end as a failed build
/// with a message for the player; the download route tries again later.
pub async fn build(state: &AppState, payload: &Value) -> Result<(), String> {
    let key = payload["content_key"].as_str().ok_or("payload without content_key")?.to_string();
    let path = zip_path(state, &key).ok_or("invalid content_key")?;
    let parts: Vec<Part> = sqlx::query_as(
        "SELECT v.asset_url, v.sha256, a.name, v.tag
         FROM pack_versions pv JOIN versions v ON v.id = pv.version_id JOIN addons a ON a.id = v.addon_id
         WHERE pv.pack_id = (SELECT id FROM packs WHERE content_key = $1 ORDER BY id LIMIT 1)
         ORDER BY lower(a.name)",
    )
    .bind(&key)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| e.to_string())?;
    if parts.is_empty() {
        return fail(state, &key, "The pack is empty").await;
    }

    let mut zips = Vec::new();
    for part in &parts {
        let bytes = match state.github.download(&part.asset_url, MAX_DOWNLOAD).await {
            Ok(bytes) => bytes,
            Err(e) => return fail(state, &key, &format!("{} {} could not be downloaded ({e}), try again later", part.name, part.tag)).await,
        };
        if part.sha256.as_deref() != Some(hex::encode(Sha256::digest(&bytes)).as_str()) {
            return fail(state, &key, &format!("{} {} changed on GitHub since it was checked", part.name, part.tag)).await;
        }
        zips.push(Cursor::new(bytes));
    }

    let max = state.config.pack_max_bytes;
    let built = tokio::task::spawn_blocking(move || -> Result<u64, String> {
        std::fs::create_dir_all(path.parent().expect("pack dir")).map_err(|e| e.to_string())?;
        let part = path.with_extension("zip.part");
        let result = std::fs::File::create(&part)
            .map_err(|e| e.to_string())
            .and_then(|file| forge_core::pack::bundle(zips, file, max).map_err(|e| e.to_string()));
        match result {
            Ok(_) => {
                std::fs::rename(&part, &path).map_err(|e| e.to_string())?;
                Ok(std::fs::metadata(&path).map_err(|e| e.to_string())?.len())
            }
            Err(e) => {
                let _ = std::fs::remove_file(&part);
                Err(e)
            }
        }
    })
    .await
    .map_err(|e| e.to_string())?;
    match built {
        Ok(size) => {
            sqlx::query("UPDATE pack_files SET status = 'ready', size = $2, error = NULL, built_at = $3 WHERE content_key = $1")
                .bind(&key)
                .bind(size as i64)
                .bind((state.clock)())
                .execute(&state.pool)
                .await
                .map_err(|e| e.to_string())?;
            Ok(())
        }
        Err(e) => fail(state, &key, &e).await,
    }
}

#[derive(sqlx::FromRow)]
struct Stored {
    content_key: String,
    status: String,
    size: Option<i64>,
    last_used: chrono::DateTime<chrono::Utc>,
}

/// Deletes zips not downloaded within the retention time, then the least recently downloaded ones
/// until all fit the storage budget. Their rows go too: the next download builds them again.
pub async fn prune(state: &AppState) -> Result<(), String> {
    let now = (state.clock)();
    let cutoff = now - Duration::days(state.config.pack_retention_days);
    let stored: Vec<Stored> = sqlx::query_as(
        "SELECT content_key, status, size, coalesce(last_download_at, built_at, requested_at) AS last_used
         FROM pack_files ORDER BY last_used DESC",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|e| e.to_string())?;
    let mut kept: u64 = 0;
    for file in stored {
        // A build is kept unless it has hung for the whole retention time.
        let building = file.status == "building";
        let over_budget = kept + file.size.unwrap_or(0) as u64 > state.config.pack_storage_bytes;
        if (building || !over_budget) && file.last_used >= cutoff {
            kept += file.size.unwrap_or(0) as u64;
            continue;
        }
        if let Some(path) = zip_path(state, &file.content_key) {
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(format!("cannot delete {}: {e}", path.display())),
            }
        }
        sqlx::query("DELETE FROM pack_files WHERE content_key = $1").bind(&file.content_key).execute(&state.pool).await.map_err(|e| e.to_string())?;
    }
    Ok(())
}
