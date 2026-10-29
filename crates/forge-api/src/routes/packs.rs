//! `/v1/packs`: addon packs, one zip with several addons for players of the browser version.
//! Any logged-in user creates them; anyone with the link sees and downloads them. The worker builds
//! the zip on the first download (`crate::pack_job`).

use crate::app::AppState;
use crate::auth::CurrentUser;
use crate::error::{ApiError, ApiResult};
use crate::jobs;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use chrono::Duration;
use rand::Rng;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// Trust levels a version needs to go into a pack; it also needs a known SHA-256.
pub const PACKABLE: &[&str] = &["scanned", "verified"];
const MAX_NAME: usize = 80;

fn invalid(code: &'static str, message: impl Into<String>) -> ApiError {
    ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, code, message)
}

/// The key of a set of versions: packs with the same versions share one zip.
pub fn content_key(version_ids: &[i64]) -> String {
    let mut ids = version_ids.to_vec();
    ids.sort_unstable();
    let joined: Vec<String> = ids.iter().map(i64::to_string).collect();
    hex::encode(Sha256::digest(joined.join(",").as_bytes()))
}

fn public_id() -> String {
    rand::rng().sample_iter(rand::distr::Alphanumeric).take(10).map(char::from).collect()
}

#[derive(Deserialize)]
pub struct NewPack {
    #[serde(default)]
    name: String,
    addons: Vec<PackAddon>,
}

#[derive(Deserialize)]
struct PackAddon {
    slug: String,
    /// Without a tag the addon's current version goes in.
    tag: Option<String>,
}

#[derive(sqlx::FromRow)]
struct Chosen {
    version_id: i64,
    name: String,
    tag: String,
    trust: String,
    sha256: Option<String>,
    size: Option<i64>,
}

pub async fn create(State(state): State<AppState>, user: CurrentUser, Json(body): Json<NewPack>) -> ApiResult<(StatusCode, Json<Value>)> {
    let name = body.name.trim();
    if name.chars().count() > MAX_NAME {
        return Err(invalid("invalid_name", "At most 80 characters"));
    }
    if body.addons.is_empty() {
        return Err(invalid("empty_pack", "Pick at least one addon"));
    }
    let max = state.config.pack_max_addons;
    if body.addons.len() as i64 > max {
        return Err(invalid("too_many_addons", format!("A pack holds at most {max} addons")));
    }
    let now = (state.clock)();
    let recent: i64 = sqlx::query_scalar("SELECT count(*) FROM packs WHERE created_by = $1 AND created_at > $2")
        .bind(user.id)
        .bind(now - Duration::hours(1))
        .fetch_one(&state.pool)
        .await?;
    if recent >= state.config.packs_per_hour {
        let message = format!("At most {} new packs an hour, try again later", state.config.packs_per_hour);
        return Err(ApiError::new(StatusCode::TOO_MANY_REQUESTS, "pack_limit", message));
    }

    let mut chosen: Vec<Chosen> = Vec::new();
    for addon in &body.addons {
        let row: Option<Chosen> = sqlx::query_as(
            "SELECT v.id AS version_id, a.name, v.tag, t.trust, v.sha256, v.asset_size AS size
             FROM addons a
             JOIN versions v ON v.addon_id = a.id AND (CASE WHEN $2::text IS NULL THEN v.id = relevant_version(a.id) ELSE v.tag = $2 END)
             JOIN version_trust t ON t.version_id = v.id
             WHERE a.slug = $1::citext AND a.deleted_at IS NULL",
        )
        .bind(&addon.slug)
        .bind(&addon.tag)
        .fetch_optional(&state.pool)
        .await?;
        let row = row.ok_or_else(|| invalid("unknown_addon", format!("No addon or version {}", addon.slug)))?;
        if !PACKABLE.contains(&row.trust.as_str()) || row.sha256.is_none() {
            return Err(invalid("not_packable", format!("{} {} is not checked or was flagged, so it cannot go into a pack", row.name, row.tag)));
        }
        if chosen.iter().any(|c| c.version_id == row.version_id) {
            return Err(invalid("duplicate_addon", format!("{} is in the list twice", row.name)));
        }
        chosen.push(row);
    }
    // Release sizes are known before the build; a source zip's size only after it.
    let known: i64 = chosen.iter().filter_map(|c| c.size).sum();
    if known as u64 > state.config.pack_max_bytes {
        return Err(invalid("too_large", format!("The pack would be larger than {} MB", state.config.pack_max_bytes / 1_000_000)));
    }

    let ids: Vec<i64> = chosen.iter().map(|c| c.version_id).collect();
    let id = public_id();
    let mut tx = state.pool.begin().await?;
    let pack: i64 = sqlx::query_scalar(
        "INSERT INTO packs (public_id, name, created_by, content_key, created_at) VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(&id)
    .bind(name)
    .bind(user.id)
    .bind(content_key(&ids))
    .bind(now)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query("INSERT INTO pack_versions (pack_id, version_id) SELECT $1, unnest($2::bigint[])")
        .bind(pack)
        .bind(&ids)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(json!({ "id": id }))))
}

#[derive(sqlx::FromRow)]
struct PackRow {
    id: i64,
    name: String,
    created_at: chrono::DateTime<chrono::Utc>,
    creator: Option<String>,
    content_key: String,
}

async fn load(state: &AppState, public_id: &str) -> ApiResult<PackRow> {
    sqlx::query_as(
        "SELECT p.id, p.name, p.created_at, u.login::text AS creator, p.content_key
         FROM packs p LEFT JOIN users u ON u.id = p.created_by WHERE p.public_id = $1",
    )
    .bind(public_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(ApiError::not_found)
}

#[derive(sqlx::FromRow, serde::Serialize)]
struct PackEntry {
    slug: String,
    name: String,
    tag: String,
    trust: String,
    /// Still installable: not deleted, blocked, flagged or changed on GitHub since.
    available: bool,
    folders: Vec<String>,
}

async fn entries(state: &AppState, pack: i64) -> sqlx::Result<Vec<PackEntry>> {
    sqlx::query_as(
        "SELECT a.slug::text AS slug, a.name, v.tag, t.trust,
                (a.deleted_at IS NULL AND v.sha256 IS NOT NULL AND t.trust = ANY($2)) AS available,
                ARRAY(SELECT folder::text FROM addon_folders WHERE addon_id = a.id AND active ORDER BY folder) AS folders
         FROM pack_versions pv
         JOIN versions v ON v.id = pv.version_id
         JOIN addons a ON a.id = v.addon_id
         JOIN version_trust t ON t.version_id = v.id
         WHERE pv.pack_id = $1
         ORDER BY lower(a.name)",
    )
    .bind(pack)
    .bind(PACKABLE)
    .fetch_all(&state.pool)
    .await
}

#[derive(sqlx::FromRow)]
struct FileRow {
    status: String,
    size: Option<i64>,
    error: Option<String>,
    requested_at: chrono::DateTime<chrono::Utc>,
}

async fn file(state: &AppState, key: &str) -> sqlx::Result<Option<FileRow>> {
    sqlx::query_as("SELECT status, size, error, requested_at FROM pack_files WHERE content_key = $1").bind(key).fetch_optional(&state.pool).await
}

/// `GET /v1/packs/{id}`: what is in the pack and whether its zip is ready.
pub async fn show(State(state): State<AppState>, Path(public_id): Path<String>) -> ApiResult<Json<Value>> {
    let pack = load(&state, &public_id).await?;
    let addons = entries(&state, pack.id).await?;
    let file = file(&state, &pack.content_key).await?;
    Ok(Json(json!({
        "id": public_id,
        "name": pack.name,
        "created_at": pack.created_at,
        "created_by": pack.creator,
        "available": addons.iter().all(|a| a.available),
        "addons": addons,
        "file": {
            "status": file.as_ref().map_or("none", |f| f.status.as_str()),
            "size": file.as_ref().and_then(|f| f.size),
            "error": file.as_ref().and_then(|f| f.error.clone()),
        },
    })))
}

/// A failed build is tried again after this long, in case GitHub was only down.
const RETRY_FAILED_AFTER: Duration = Duration::hours(1);

/// `GET /v1/packs/{id}/download`: a redirect to the zip when it is built, else `202` while the
/// worker builds it (the client asks again).
pub async fn download(State(state): State<AppState>, Path(public_id): Path<String>) -> ApiResult<Response> {
    let pack = load(&state, &public_id).await?;
    if !entries(&state, pack.id).await?.iter().all(|a| a.available) {
        return Err(ApiError::new(StatusCode::CONFLICT, "pack_unavailable", "An addon of this pack is no longer available"));
    }
    let key = &pack.content_key;
    let now = (state.clock)();
    match file(&state, key).await? {
        Some(f) if f.status == "ready" => {
            sqlx::query("UPDATE pack_files SET last_download_at = $2 WHERE content_key = $1").bind(key).bind(now).execute(&state.pool).await?;
            return Ok(Redirect::to(&format!("{}/{key}.zip", state.config.public_pack_url)).into_response());
        }
        Some(f) if f.status == "building" => {}
        Some(f) if f.status == "failed" && now - f.requested_at < RETRY_FAILED_AFTER => {
            let message = f.error.unwrap_or_else(|| "The pack could not be built".into());
            return Err(ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, "build_failed", message));
        }
        _ => {
            sqlx::query(
                "INSERT INTO pack_files (content_key, status, requested_at) VALUES ($1, 'building', $2)
                 ON CONFLICT (content_key) DO UPDATE SET status = 'building', error = NULL, requested_at = $2",
            )
            .bind(key)
            .bind(now)
            .execute(&state.pool)
            .await?;
            jobs::enqueue(&state.pool, crate::pack_job::KIND, json!({ "content_key": key }), now, Some(&format!("pack:{key}"))).await?;
        }
    }
    Ok((StatusCode::ACCEPTED, Json(json!({ "status": "building" }))).into_response())
}
