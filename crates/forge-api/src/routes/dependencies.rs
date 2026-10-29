//! `PUT /v1/addons/{slug}/dependencies`: the owner replaces the addon's dependency list.

use super::addons::{addon_view, load_addon, not_blocked, require, user_permissions};
use crate::app::AppState;
use crate::auth::CurrentUser;
use crate::error::{ApiError, ApiResult};
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use forge_core::deps::{find_cycle, install_order};
use serde::Deserialize;
use serde_json::Value;
use sqlx::PgPool;
use std::collections::HashMap;

#[derive(Deserialize)]
pub struct Dependency {
    slug: String,
    #[serde(default = "yes")]
    required: bool,
}

fn yes() -> bool {
    true
}

fn invalid(code: &'static str, message: impl Into<String>) -> ApiError {
    ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, code, message)
}

/// Every dependency edge between addons that are not deleted. `only_required` drops optional ones.
async fn edges(pool: &PgPool, only_required: bool) -> sqlx::Result<HashMap<i64, Vec<i64>>> {
    let rows: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT d.addon_id, d.depends_on_id FROM addon_dependencies d
         JOIN addons a ON a.id = d.addon_id JOIN addons b ON b.id = d.depends_on_id
         WHERE a.deleted_at IS NULL AND b.deleted_at IS NULL AND (d.required OR NOT $1)",
    )
    .bind(only_required)
    .fetch_all(pool)
    .await?;
    let mut map: HashMap<i64, Vec<i64>> = HashMap::new();
    for (from, to) in rows {
        map.entry(from).or_default().push(to);
    }
    Ok(map)
}

/// The slugs of everything `addon_id` needs, in install order.
pub async fn required_order(pool: &PgPool, addon_id: i64) -> sqlx::Result<Vec<String>> {
    let order = install_order(&addon_id, &edges(pool, true).await?);
    let slugs: Vec<(i64, String)> = sqlx::query_as("SELECT id, slug::text FROM addons WHERE id = ANY($1)").bind(&order).fetch_all(pool).await?;
    let by_id: HashMap<i64, String> = slugs.into_iter().collect();
    Ok(order.iter().filter_map(|id| by_id.get(id).cloned()).collect())
}

pub async fn put(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(slug): Path<String>,
    Json(body): Json<Vec<Dependency>>,
) -> ApiResult<Json<Value>> {
    let addon = load_addon(&state.pool, &slug).await?;
    not_blocked(&addon)?;
    require(user_permissions(&state.pool, &addon, &user).await?.owner, "Only the owner sets dependencies")?;

    let mut targets: Vec<(i64, String, bool)> = Vec::new();
    for dep in &body {
        if dep.slug.eq_ignore_ascii_case(&addon.slug) {
            return Err(invalid("self_dependency", "An addon cannot depend on itself"));
        }
        let id: Option<i64> = sqlx::query_scalar("SELECT id FROM addons WHERE slug = $1::citext AND deleted_at IS NULL")
            .bind(&dep.slug)
            .fetch_optional(&state.pool)
            .await?;
        let id = id.ok_or_else(|| invalid("unknown_dependency", format!("There is no addon {}", dep.slug)))?;
        if targets.iter().any(|(t, _, _)| *t == id) {
            return Err(invalid("duplicate_dependency", format!("{} is listed twice", dep.slug)));
        }
        targets.push((id, dep.slug.clone(), dep.required));
    }

    let mut graph = edges(&state.pool, false).await?;
    graph.remove(&addon.id);
    for (to, _, _) in &targets {
        if let Some(cycle) = find_cycle(&graph, &addon.id, to) {
            let names: Vec<(i64, String)> =
                sqlx::query_as("SELECT id, slug::text FROM addons WHERE id = ANY($1)").bind(&cycle).fetch_all(&state.pool).await?;
            let names: HashMap<i64, String> = names.into_iter().collect();
            let path: Vec<&str> = cycle.iter().map(|id| names.get(id).map(String::as_str).unwrap_or("?")).collect();
            return Err(invalid("dependency_cycle", format!("This would make a cycle: {}", path.join(" -> "))));
        }
        graph.entry(addon.id).or_default().push(*to);
    }

    let mut tx = state.pool.begin().await?;
    sqlx::query("DELETE FROM addon_dependencies WHERE addon_id = $1").bind(addon.id).execute(&mut *tx).await?;
    for (to, _, required) in &targets {
        sqlx::query("INSERT INTO addon_dependencies (addon_id, depends_on_id, required) VALUES ($1, $2, $3)")
            .bind(addon.id)
            .bind(to)
            .bind(required)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(Json(addon_view(&state, addon.id).await?))
}
