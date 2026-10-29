//! The creator pages: my addons, publishing an addon and everything an owner or contributor
//! changes on it. The API enforces every right; these pages only show what the visitor may do.

use crate::app::{AppState, visitor};
use crate::csrf::CsrfToken;
use crate::error::{WebError, WebResult};
use crate::templates::{
    DeleteAddonPage, DepsPage, EditAddonPage, Errors, ImagesPage, MembersPage, MyAddonsPage, NewAddonPage, Perms, VersionEditPage, Visitor, render,
};
use axum::Extension;
use axum::extract::{Form, Multipart, Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum_extra::extract::PrivateCookieJar;
use reqwest::Method;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;

use crate::api_client::ApiError;

/// The logged-in visitor or the login page.
async fn me(state: &AppState, jar: &PrivateCookieJar) -> WebResult<(Visitor, String)> {
    visitor(state, jar).await?.ok_or(WebError::LoginRequired)
}

fn s(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Which form field an API error belongs to.
fn field_of(code: &str) -> &'static str {
    match code {
        "invalid_slug" | "slug_taken" => "slug",
        "invalid_name" => "name",
        "invalid_summary" => "summary",
        "description_too_long" => "description",
        "invalid_repo" | "app_not_installed" | "repo_private" | "not_repo_admin" | "repo_taken" | "repo_not_found" => "repo",
        "invalid_asset_pattern" | "invalid_source_type" => "asset_pattern",
        "invalid_folder" | "folder_taken" | "folder_reserved" => "folders",
        "unknown_category" => "category",
        _ => "form",
    }
}

/// An API refusal as a message on its field; anything else stays an error page.
fn form_errors(e: ApiError) -> WebResult<Errors> {
    match e {
        ApiError::Api { status, code, message } if status < 500 && status != 404 => {
            Ok(Errors(HashMap::from([(field_of(&code).to_string(), message)])))
        }
        other => Err(other.into()),
    }
}

fn folders_of(text: &str) -> Vec<String> {
    text.split([',', '\n']).map(str::trim).filter(|f| !f.is_empty()).map(str::to_string).collect()
}

async fn categories(state: &AppState) -> WebResult<Vec<crate::templates::CategoryOption>> {
    crate::app::selectable_categories(state).await
}

async fn perms(state: &AppState, slug: &str, token: &str) -> WebResult<Perms> {
    let p = state.api.get(&format!("/v1/addons/{slug}/permissions"), Some(token)).await?;
    Ok(Perms {
        owner: p["owner"] == true,
        edit_description: p["edit_description"] == true,
        edit_images: p["edit_images"] == true,
    })
}

fn forbidden() -> Response {
    crate::error::error_page(StatusCode::FORBIDDEN, "Not allowed", "You may not change this part of the addon.")
}

pub async fn my_addons(State(state): State<AppState>, jar: PrivateCookieJar, Extension(csrf): Extension<CsrfToken>) -> WebResult<Html<String>> {
    let (user, token) = me(&state, &jar).await?;
    let list = state.api.get("/v1/me/addons", Some(&token)).await?;
    let rows = list
        .as_array()
        .into_iter()
        .flatten()
        .map(|a| crate::templates::MyAddon {
            slug: s(&a["slug"]),
            name: s(&a["name"]),
            role: s(&a["role"]),
            notice: crate::trust::notice(a["trust"].as_str().unwrap_or("unscanned"), false, false),
            has_version: !a["latest_tag"].is_null(),
            findings: a["findings"]["critical"].as_i64().unwrap_or(0) + a["findings"]["warn"].as_i64().unwrap_or(0),
            blocked: a["blocked"] == true,
            locked: a["locked"] == true,
        })
        .collect();
    Ok(render(&MyAddonsPage { user: Some(user), csrf: csrf.0, rows }))
}

#[derive(Deserialize, Default, Clone)]
pub struct NewAddonForm {
    #[serde(default)]
    pub repo: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub source_type: String,
    #[serde(default)]
    pub asset_pattern: String,
    #[serde(default)]
    pub folders: String,
}

pub async fn new_addon(State(state): State<AppState>, jar: PrivateCookieJar, Extension(csrf): Extension<CsrfToken>) -> WebResult<Html<String>> {
    let (user, _) = me(&state, &jar).await?;
    let form = NewAddonForm { source_type: "release".into(), ..Default::default() };
    Ok(render(&NewAddonPage {
        user: Some(user),
        csrf: csrf.0,
        install_url: state.install_url(),
        categories: categories(&state).await?,
        form,
        errors: Errors::default(),
    }))
}

pub async fn create_addon(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Extension(csrf): Extension<CsrfToken>,
    Form(form): Form<NewAddonForm>,
) -> WebResult<Response> {
    let (user, token) = me(&state, &jar).await?;
    let body = json!({
        "repo": form.repo.trim(), "slug": form.slug.trim(), "name": form.name.trim(), "summary": form.summary.trim(),
        "category": form.category, "source_type": form.source_type, "asset_pattern": form.asset_pattern.trim(),
        "folders": folders_of(&form.folders),
    });
    match state.api.post("/v1/addons", Some(&token), &body).await {
        Ok(created) => Ok(Redirect::to(&format!("/addons/{}/edit", s(&created["slug"]))).into_response()),
        Err(e) => {
            let errors = form_errors(e)?;
            let page = NewAddonPage { user: Some(user), csrf: csrf.0, install_url: state.install_url(), categories: categories(&state).await?, form, errors };
            Ok((StatusCode::UNPROCESSABLE_ENTITY, render(&page)).into_response())
        }
    }
}

#[derive(Deserialize, Default)]
pub struct EditForm {
    pub name: Option<String>,
    pub category: Option<String>,
    pub source_type: Option<String>,
    pub asset_pattern: Option<String>,
    pub folders: Option<String>,
    pub summary: Option<String>,
    pub description_md: Option<String>,
}

async fn edit_page(state: &AppState, user: Visitor, token: &str, csrf: String, slug: &str, errors: Errors, saved: bool) -> WebResult<Html<String>> {
    let p = perms(state, slug, token).await?;
    let a = state.api.get(&format!("/v1/addons/{slug}"), None).await?;
    let description = s(&a["description_md"]);
    Ok(render(&EditAddonPage {
        user: Some(user),
        csrf,
        slug: slug.to_string(),
        name: s(&a["name"]),
        category: s(&a["category"]),
        categories: categories(state).await?,
        source_type: s(&a["source_type"]),
        asset_pattern: s(&a["asset_pattern"]),
        folders: a["folders"].as_array().into_iter().flatten().map(s).collect::<Vec<_>>().join("\n"),
        summary: s(&a["summary"]),
        description_html: forge_core::markdown::render(&description, &state.image_base()),
        description,
        perms: p,
        errors,
        saved,
    }))
}

pub async fn edit_addon(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Extension(csrf): Extension<CsrfToken>,
    Path(slug): Path<String>,
) -> WebResult<Response> {
    let (user, token) = me(&state, &jar).await?;
    let p = perms(&state, &slug, &token).await?;
    if !p.owner && !p.edit_description && !p.edit_images {
        return Ok(forbidden());
    }
    if !p.owner && !p.edit_description {
        return Ok(Redirect::to(&format!("/addons/{slug}/images")).into_response());
    }
    Ok(edit_page(&state, user, &token, csrf.0, &slug, Errors::default(), false).await?.into_response())
}

pub async fn save_addon(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Extension(csrf): Extension<CsrfToken>,
    Path(slug): Path<String>,
    Form(form): Form<EditForm>,
) -> WebResult<Response> {
    let (user, token) = me(&state, &jar).await?;
    let mut body = serde_json::Map::new();
    let mut put = |key: &str, value: &Option<String>| {
        if let Some(v) = value {
            body.insert(key.into(), json!(v.trim()));
        }
    };
    put("name", &form.name);
    put("category", &form.category);
    put("source_type", &form.source_type);
    put("asset_pattern", &form.asset_pattern);
    put("summary", &form.summary);
    if let Some(d) = &form.description_md {
        body.insert("description_md".into(), json!(d));
    }
    if let Some(f) = &form.folders {
        body.insert("folders".into(), json!(folders_of(f)));
    }
    match state.api.call(Method::PATCH, &format!("/v1/addons/{slug}"), Some(&token), Some(&Value::Object(body))).await {
        Ok(_) => Ok(edit_page(&state, user, &token, csrf.0, &slug, Errors::default(), true).await?.into_response()),
        Err(ApiError::Api { status: 403, .. }) => Ok(forbidden()),
        Err(e) => {
            let errors = form_errors(e)?;
            Ok((StatusCode::UNPROCESSABLE_ENTITY, edit_page(&state, user, &token, csrf.0, &slug, errors, false).await?).into_response())
        }
    }
}

#[derive(Deserialize)]
pub struct PreviewForm {
    #[serde(default)]
    description_md: String,
}

/// The rendered description for the editor's preview; the same sanitizer as the API.
pub async fn preview(State(state): State<AppState>, jar: PrivateCookieJar, Form(form): Form<PreviewForm>) -> WebResult<Html<String>> {
    me(&state, &jar).await?;
    Ok(Html(forge_core::markdown::render(&form.description_md, &state.image_base())))
}

async fn images_page(state: &AppState, user: Visitor, csrf: String, slug: &str, message: Option<String>, error: Option<String>) -> WebResult<Html<String>> {
    let a = state.api.get(&format!("/v1/addons/{slug}"), None).await?;
    let images = a["images"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|i| crate::templates::ImageRow { id: i["id"].as_i64().unwrap_or(0), url: s(&i["url"]), caption: s(&i["caption"]) })
        .collect();
    Ok(render(&ImagesPage { user: Some(user), csrf, slug: slug.into(), name: s(&a["name"]), icon_url: a["icon_url"].as_str().map(str::to_string), images, message, error }))
}

pub async fn images(State(state): State<AppState>, jar: PrivateCookieJar, Extension(csrf): Extension<CsrfToken>, Path(slug): Path<String>) -> WebResult<Response> {
    let (user, token) = me(&state, &jar).await?;
    if !perms(&state, &slug, &token).await?.edit_images {
        return Ok(forbidden());
    }
    Ok(images_page(&state, user, csrf.0, &slug, None, None).await?.into_response())
}

pub async fn upload_image(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Extension(csrf): Extension<CsrfToken>,
    Path(slug): Path<String>,
    mut multipart: Multipart,
) -> WebResult<Response> {
    let (user, token) = me(&state, &jar).await?;
    let mut form = reqwest::multipart::Form::new();
    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or_default().to_string();
        let file_name = field.file_name().unwrap_or("upload").to_string();
        let Ok(bytes) = field.bytes().await else {
            return Ok(images_page(&state, user, csrf.0, &slug, None, Some("Images may be at most 5 MB.".into())).await?.into_response());
        };
        form = match name.as_str() {
            "file" => form.part("file", reqwest::multipart::Part::bytes(bytes.to_vec()).file_name(file_name)),
            "kind" | "caption" => form.text(name, String::from_utf8_lossy(&bytes).into_owned()),
            _ => form,
        };
    }
    let (message, error) = match state.api.post_multipart(&format!("/v1/addons/{slug}/images"), &token, form).await {
        Ok(answer) if answer["notice"] == "resized" => (
            Some(format!(
                "Image was scaled down from {} x {} to {} x {} px.",
                answer["original"]["width"], answer["original"]["height"], answer["stored"]["width"], answer["stored"]["height"]
            )),
            None,
        ),
        Ok(_) => (Some("Image uploaded.".into()), None),
        Err(ApiError::Api { message, .. }) => (None, Some(message)),
        Err(e) => return Err(e.into()),
    };
    Ok(images_page(&state, user, csrf.0, &slug, message, error).await?.into_response())
}

#[derive(Deserialize)]
pub struct MoveForm {
    direction: String,
}

/// Swaps an image with its neighbour; positions are the order the API returns them in.
pub async fn move_image(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path((slug, id)): Path<(String, i64)>,
    Form(form): Form<MoveForm>,
) -> WebResult<Response> {
    let (_, token) = me(&state, &jar).await?;
    let a = state.api.get(&format!("/v1/addons/{slug}"), None).await?;
    let ids: Vec<i64> = a["images"].as_array().into_iter().flatten().filter_map(|i| i["id"].as_i64()).collect();
    if let Some(at) = ids.iter().position(|i| *i == id) {
        let other = if form.direction == "up" { at.checked_sub(1) } else { Some(at + 1).filter(|o| *o < ids.len()) };
        if let Some(other) = other {
            let mut order = ids.clone();
            order.swap(at, other);
            for (position, image) in order.iter().enumerate() {
                state.api.call(Method::PATCH, &format!("/v1/addons/{slug}/images/{image}"), Some(&token), Some(&json!({ "position": position }))).await?;
            }
        }
    }
    Ok(Redirect::to(&format!("/addons/{slug}/images")).into_response())
}

pub async fn delete_image(State(state): State<AppState>, jar: PrivateCookieJar, Path((slug, id)): Path<(String, i64)>) -> WebResult<Response> {
    let (_, token) = me(&state, &jar).await?;
    state.api.call(Method::DELETE, &format!("/v1/addons/{slug}/images/{id}"), Some(&token), None).await?;
    Ok(Redirect::to(&format!("/addons/{slug}/images")).into_response())
}

async fn members_page(state: &AppState, user: Visitor, token: &str, csrf: String, slug: &str, error: Option<String>) -> WebResult<Html<String>> {
    let list = state.api.get(&format!("/v1/addons/{slug}/members"), Some(token)).await?;
    let members = list
        .as_array()
        .into_iter()
        .flatten()
        .map(|m| crate::templates::MemberRow {
            github_id: m["github_id"].as_i64().unwrap_or(0),
            login: s(&m["login"]),
            description: m["can_edit_description"] == true,
            images: m["can_edit_images"] == true,
        })
        .collect();
    Ok(render(&MembersPage { user: Some(user), csrf, slug: slug.into(), members, error }))
}

pub async fn members(State(state): State<AppState>, jar: PrivateCookieJar, Extension(csrf): Extension<CsrfToken>, Path(slug): Path<String>) -> WebResult<Response> {
    let (user, token) = me(&state, &jar).await?;
    if !perms(&state, &slug, &token).await?.owner {
        return Ok(forbidden());
    }
    Ok(members_page(&state, user, &token, csrf.0, &slug, None).await?.into_response())
}

#[derive(Deserialize)]
pub struct MemberForm {
    #[serde(default)]
    login: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    images: Option<String>,
}

pub async fn add_member(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Extension(csrf): Extension<CsrfToken>,
    Path(slug): Path<String>,
    Form(form): Form<MemberForm>,
) -> WebResult<Response> {
    let (user, token) = me(&state, &jar).await?;
    let body = json!({ "login": form.login.trim(), "can_edit_description": form.description.is_some(), "can_edit_images": form.images.is_some() });
    match state.api.post(&format!("/v1/addons/{slug}/members"), Some(&token), &body).await {
        Ok(_) => Ok(Redirect::to(&format!("/addons/{slug}/members")).into_response()),
        Err(ApiError::Api { message, status, .. }) if status != 403 => Ok(members_page(&state, user, &token, csrf.0, &slug, Some(message)).await?.into_response()),
        Err(e) => Err(e.into()),
    }
}

pub async fn update_member(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path((slug, github_id)): Path<(String, i64)>,
    Form(form): Form<MemberForm>,
) -> WebResult<Response> {
    let (_, token) = me(&state, &jar).await?;
    let body = json!({ "can_edit_description": form.description.is_some(), "can_edit_images": form.images.is_some() });
    state.api.call(Method::PATCH, &format!("/v1/addons/{slug}/members/{github_id}"), Some(&token), Some(&body)).await?;
    Ok(Redirect::to(&format!("/addons/{slug}/members")).into_response())
}

pub async fn remove_member(State(state): State<AppState>, jar: PrivateCookieJar, Path((slug, github_id)): Path<(String, i64)>) -> WebResult<Response> {
    let (_, token) = me(&state, &jar).await?;
    state.api.call(Method::DELETE, &format!("/v1/addons/{slug}/members/{github_id}"), Some(&token), None).await?;
    Ok(Redirect::to(&format!("/addons/{slug}/members")).into_response())
}

fn current_deps(addon: &Value) -> Vec<(String, String, bool)> {
    addon["dependencies"].as_array().into_iter().flatten().map(|d| (s(&d["slug"]), s(&d["name"]), d["required"] == true)).collect()
}

async fn deps_page(state: &AppState, user: Visitor, csrf: String, slug: &str, error: Option<String>) -> WebResult<Html<String>> {
    let a = state.api.get(&format!("/v1/addons/{slug}"), None).await?;
    Ok(render(&DepsPage { user: Some(user), csrf, slug: slug.into(), deps: current_deps(&a), error }))
}

pub async fn deps(State(state): State<AppState>, jar: PrivateCookieJar, Extension(csrf): Extension<CsrfToken>, Path(slug): Path<String>) -> WebResult<Response> {
    let (user, token) = me(&state, &jar).await?;
    if !perms(&state, &slug, &token).await?.owner {
        return Ok(forbidden());
    }
    Ok(deps_page(&state, user, csrf.0, &slug, None).await?.into_response())
}

#[derive(Deserialize)]
pub struct DepForm {
    #[serde(default)]
    dependency: String,
    #[serde(default)]
    required: Option<String>,
    #[serde(default)]
    remove: Option<String>,
}

pub async fn save_deps(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Extension(csrf): Extension<CsrfToken>,
    Path(slug): Path<String>,
    Form(form): Form<DepForm>,
) -> WebResult<Response> {
    let (user, token) = me(&state, &jar).await?;
    let a = state.api.get(&format!("/v1/addons/{slug}"), None).await?;
    let mut list: Vec<Value> = current_deps(&a)
        .into_iter()
        .filter(|(d, _, _)| Some(d) != form.remove.as_ref())
        .map(|(d, _, required)| json!({ "slug": d, "required": required }))
        .collect();
    if !form.dependency.trim().is_empty() {
        list.push(json!({ "slug": form.dependency.trim(), "required": form.required.is_some() }));
    }
    match state.api.call(Method::PUT, &format!("/v1/addons/{slug}/dependencies"), Some(&token), Some(&Value::Array(list))).await {
        Ok(_) => Ok(Redirect::to(&format!("/addons/{slug}/dependencies")).into_response()),
        Err(ApiError::Api { message, status, .. }) if status != 403 => Ok(deps_page(&state, user, csrf.0, &slug, Some(message)).await?.into_response()),
        Err(e) => Err(e.into()),
    }
}

pub async fn version_edit(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Extension(csrf): Extension<CsrfToken>,
    Path((slug, tag)): Path<(String, String)>,
) -> WebResult<Response> {
    let (user, token) = me(&state, &jar).await?;
    if !perms(&state, &slug, &token).await?.owner {
        return Ok(forbidden());
    }
    let versions = state.api.get(&format!("/v1/addons/{slug}/versions"), None).await?;
    let v = versions.as_array().into_iter().flatten().find(|v| v["tag"] == tag.as_str()).ok_or(WebError::NotFound)?.clone();
    let games: Vec<String> = state.api.get("/v1/game-versions", None).await?.as_array().into_iter().flatten().map(|g| s(&g["name"])).collect();
    Ok(render(&VersionEditPage { user: Some(user), csrf: csrf.0, slug, tag, game_min: s(&v["game_min"]), game_max: s(&v["game_max"]), games, error: None })
        .into_response())
}

#[derive(Deserialize)]
pub struct RangeForm {
    #[serde(default)]
    game_min: String,
    #[serde(default)]
    game_max: String,
}

pub async fn save_version(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Extension(csrf): Extension<CsrfToken>,
    Path((slug, tag)): Path<(String, String)>,
    Form(form): Form<RangeForm>,
) -> WebResult<Response> {
    let (user, token) = me(&state, &jar).await?;
    let opt = |v: &str| if v.is_empty() { Value::Null } else { json!(v) };
    let body = json!({ "game_min": opt(&form.game_min), "game_max": opt(&form.game_max) });
    match state.api.call(Method::PATCH, &format!("/v1/addons/{slug}/versions/{tag}"), Some(&token), Some(&body)).await {
        Ok(_) => Ok(Redirect::to(&format!("/addons/{slug}/versions")).into_response()),
        Err(ApiError::Api { message, status, .. }) if status != 403 && status != 404 => {
            let games: Vec<String> = state.api.get("/v1/game-versions", None).await?.as_array().into_iter().flatten().map(|g| s(&g["name"])).collect();
            let page = VersionEditPage { user: Some(user), csrf: csrf.0, slug, tag, game_min: form.game_min, game_max: form.game_max, games, error: Some(message) };
            Ok((StatusCode::UNPROCESSABLE_ENTITY, render(&page)).into_response())
        }
        Err(e) => Err(e.into()),
    }
}

pub async fn delete_page(State(state): State<AppState>, jar: PrivateCookieJar, Extension(csrf): Extension<CsrfToken>, Path(slug): Path<String>) -> WebResult<Response> {
    let (user, token) = me(&state, &jar).await?;
    if !perms(&state, &slug, &token).await?.owner {
        return Ok(forbidden());
    }
    Ok(render(&DeleteAddonPage { user: Some(user), csrf: csrf.0, slug, error: None }).into_response())
}

#[derive(Deserialize)]
pub struct DeleteForm {
    #[serde(default)]
    confirm: String,
}

pub async fn delete_addon(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Extension(csrf): Extension<CsrfToken>,
    Path(slug): Path<String>,
    Form(form): Form<DeleteForm>,
) -> WebResult<Response> {
    let (user, token) = me(&state, &jar).await?;
    if form.confirm.trim() != slug {
        let page = DeleteAddonPage { user: Some(user), csrf: csrf.0, slug, error: Some("Type the slug exactly to confirm.".into()) };
        return Ok((StatusCode::UNPROCESSABLE_ENTITY, render(&page)).into_response());
    }
    state.api.call(Method::DELETE, &format!("/v1/addons/{slug}"), Some(&token), None).await?;
    Ok(Redirect::to("/me/addons").into_response())
}
