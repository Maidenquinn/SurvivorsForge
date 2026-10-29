//! `POST /v1/github/webhook`: events of the GitHub App. The handler checks the signature, records
//! the delivery and queues work; it never calls GitHub itself.

use crate::app::AppState;
use crate::error::{ApiError, ApiResult};
use crate::sync::enqueue_sync;
use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use hmac::{Hmac, KeyInit, Mac};
use serde_json::{Value, json};
use sha2::Sha256;

/// Checks `X-Hub-Signature-256: sha256=<hex>` in constant time.
pub fn signature_valid(secret: &str, body: &[u8], header: Option<&str>) -> bool {
    let Some(hex_sig) = header.and_then(|h| h.strip_prefix("sha256=")) else {
        return false;
    };
    let Ok(expected) = hex::decode(hex_sig) else {
        return false;
    };
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC takes any key length");
    mac.update(body);
    mac.verify_slice(&expected).is_ok()
}

fn header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|v| v.to_str().ok())
}

pub async fn webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    if !signature_valid(
        &state.config.github_webhook_secret,
        &body,
        header(&headers, "x-hub-signature-256"),
    ) {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "bad_signature",
            "Invalid webhook signature",
        ));
    }
    let event = header(&headers, "x-github-event")
        .unwrap_or_default()
        .to_string();
    let delivery = header(&headers, "x-github-delivery")
        .unwrap_or_default()
        .to_string();
    let now = (state.clock)();
    let fresh = sqlx::query("INSERT INTO webhook_deliveries (delivery_id, event, received_at) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING")
        .bind(&delivery)
        .bind(&event)
        .bind(now)
        .execute(&state.pool)
        .await?
        .rows_affected()
        == 1;
    if !fresh {
        return Ok(Json(json!({ "status": "duplicate" })));
    }
    let payload: Value = serde_json::from_slice(&body).map_err(|_| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            "bad_payload",
            "The payload is not JSON",
        )
    })?;
    let action = payload["action"].as_str().unwrap_or_default();
    let repo_id = payload["repository"]["id"].as_i64();

    match (event.as_str(), action) {
        ("release", _) | ("create", _) => {
            if let Some(repo_id) = repo_id {
                for addon in addons_of(&state, &[repo_id]).await? {
                    enqueue_sync(&state, addon, now).await?;
                }
            }
        }
        ("repository", "renamed" | "transferred") => {
            sqlx::query(
                "UPDATE addons SET repo_full_name = $2 WHERE repo_id = $1 AND deleted_at IS NULL",
            )
            .bind(repo_id)
            .bind(
                payload["repository"]["full_name"]
                    .as_str()
                    .unwrap_or_default(),
            )
            .execute(&state.pool)
            .await?;
        }
        ("repository", "privatized" | "deleted") => {
            lock(&state, &repo_ids(repo_id), &format!("repository {action}")).await?
        }
        ("repository", "publicized") => unlock(&state, &repo_ids(repo_id), None).await?,
        ("installation", "deleted") => {
            let installation = payload["installation"]["id"].as_i64();
            sqlx::query(
                "UPDATE addons SET locked_at = coalesce(locked_at, now()), locked_reason = $2 WHERE installation_id = $1 AND deleted_at IS NULL",
            )
            .bind(installation)
            .bind(format!("GitHub App {action}"))
            .execute(&state.pool)
            .await?;
            mark_missing(&state, None, installation).await?;
        }
        ("installation_repositories", "removed") => {
            lock(
                &state,
                &ids(&payload["repositories_removed"]),
                "GitHub App removed from the repo",
            )
            .await?;
        }
        ("installation_repositories", "added") | ("installation", "created") => {
            let installation = payload["installation"]["id"].as_i64();
            let repos = if event == "installation" {
                &payload["repositories"]
            } else {
                &payload["repositories_added"]
            };
            unlock(&state, &ids(repos), installation).await?;
        }
        _ => {}
    }
    Ok(Json(json!({ "status": "ok" })))
}

fn repo_ids(repo_id: Option<i64>) -> Vec<i64> {
    repo_id.into_iter().collect()
}

fn ids(list: &Value) -> Vec<i64> {
    list.as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| r["id"].as_i64())
        .collect()
}

async fn addons_of(state: &AppState, repo_ids: &[i64]) -> sqlx::Result<Vec<i64>> {
    sqlx::query_scalar("SELECT id FROM addons WHERE repo_id = ANY($1) AND deleted_at IS NULL AND locked_at IS NULL")
        .bind(repo_ids)
        .fetch_all(&state.pool)
        .await
}

/// No new versions while locked; the existing ones count as no longer downloadable.
async fn lock(state: &AppState, repo_ids: &[i64], reason: &str) -> sqlx::Result<()> {
    sqlx::query("UPDATE addons SET locked_at = coalesce(locked_at, now()), locked_reason = $2 WHERE repo_id = ANY($1) AND deleted_at IS NULL")
        .bind(repo_ids)
        .bind(reason)
        .execute(&state.pool)
        .await?;
    mark_missing(state, Some(repo_ids), None).await
}

async fn mark_missing(
    state: &AppState,
    repo_ids: Option<&[i64]>,
    installation: Option<i64>,
) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE versions v SET source_status = 'missing' FROM addons a
         WHERE a.id = v.addon_id AND a.deleted_at IS NULL AND (a.repo_id = ANY($1) OR a.installation_id = $2)",
    )
    .bind(repo_ids.unwrap_or_default())
    .bind(installation)
    .execute(&state.pool)
    .await?;
    Ok(())
}

/// The repo is reachable again: lift the lock and re-sync, which also finds out what is still there.
async fn unlock(state: &AppState, repo_ids: &[i64], installation: Option<i64>) -> sqlx::Result<()> {
    let unlocked: Vec<i64> = sqlx::query_scalar(
        "UPDATE addons SET locked_at = NULL, locked_reason = NULL, installation_id = coalesce($2, installation_id)
         WHERE repo_id = ANY($1) AND deleted_at IS NULL AND locked_at IS NOT NULL RETURNING id",
    )
    .bind(repo_ids)
    .bind(installation)
    .fetch_all(&state.pool)
    .await?;
    let now = (state.clock)();
    for id in unlocked {
        sqlx::query("UPDATE versions SET source_status = 'available' WHERE addon_id = $1")
            .bind(id)
            .execute(&state.pool)
            .await?;
        enqueue_sync(state, id, now).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sign(secret: &str, body: &[u8]) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
    }

    #[test]
    fn signature_checks() {
        let body = br#"{"action":"published"}"#;
        assert!(signature_valid("s3cret", body, Some(&sign("s3cret", body))));
        assert!(!signature_valid("s3cret", body, Some(&sign("other", body))));
        assert!(!signature_valid(
            "s3cret",
            b"tampered",
            Some(&sign("s3cret", body))
        ));
        assert!(!signature_valid("s3cret", body, None));
        assert!(!signature_valid("s3cret", body, Some("sha256=zz")));
        assert!(!signature_valid(
            "s3cret",
            body,
            Some(&sign("s3cret", body).replace("sha256=", "sha1="))
        ));
    }
}
