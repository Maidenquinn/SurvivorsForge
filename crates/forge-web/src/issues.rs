//! Server-rendered community issues, proxied through forge-api.
use crate::app::{AppState, visitor};
use crate::csrf::CsrfToken;
use crate::error::{WebError, WebResult};
use crate::templates::{IssuesPage, IssuePage, IssueItem, IssueComment, render};
use axum::{Extension, extract::{Path, Form, State}, response::{Html, Redirect}};
use axum_extra::extract::PrivateCookieJar;
use serde::Deserialize;
use serde_json::{Value, json};

fn s(v: &Value) -> String { v.as_str().unwrap_or_default().to_owned() }
fn segment(v: &str) -> String {
    v.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
        (b as char).to_string()
    } else { format!("%{b:02X}") }).collect()
}
#[derive(Deserialize)]
pub struct IssueForm { title: String, body: String, kind: String }
#[derive(Deserialize)]
pub struct CommentForm { body: String }
#[derive(Deserialize)]
pub struct StatusForm { status: String }

pub async fn list(State(state): State<AppState>, jar: PrivateCookieJar, Extension(csrf): Extension<CsrfToken>,
    Path(slug): Path<String>) -> WebResult<Html<String>> {
    let user = visitor(&state, &jar).await?.map(|(v,_)|v);
    let a = state.api.get(&format!("/v1/addons/{}",segment(&slug)),None).await?;
    if a["blocked"] == true { return Err(WebError::NotFound); }
    let data = state.api.get(&format!("/v1/addons/{}/issues",segment(&slug)),None).await?;
    let issues = data.as_array().into_iter().flatten().map(|i| IssueItem {
        id: i["id"].as_i64().unwrap_or(0), title:s(&i["title"]), kind:s(&i["kind"]),
        status:s(&i["status"]), author:s(&i["author"]),
    }).collect();
    Ok(render(&IssuesPage { user, csrf:csrf.0, slug, name:s(&a["name"]), issues }))
}
pub async fn create(State(state): State<AppState>, jar: PrivateCookieJar, Path(slug): Path<String>,
    Form(form): Form<IssueForm>) -> WebResult<Redirect> {
    let (_,token) = visitor(&state,&jar).await?.ok_or(WebError::LoginRequired)?;
    let result = state.api.post(&format!("/v1/addons/{}/issues",segment(&slug)),Some(&token),
        &json!({"title":form.title,"body":form.body,"kind":form.kind})).await?;
    Ok(Redirect::to(&format!("/addons/{}?tab=issues&issue={}#issue-detail",segment(&slug),result["id"].as_i64().unwrap_or(0))))
}
pub async fn show(State(state): State<AppState>, jar: PrivateCookieJar, Extension(csrf): Extension<CsrfToken>,
    Path((slug,id)): Path<(String,i64)>) -> WebResult<Html<String>> {
    let session = visitor(&state,&jar).await?;
    let user = session.as_ref().map(|(u,_)|u.clone());
    let a = state.api.get(&format!("/v1/addons/{}",segment(&slug)),None).await?;
    if a["blocked"] == true { return Err(WebError::NotFound); }
    let i = state.api.get(&format!("/v1/addons/{}/issues/{id}",segment(&slug)),None).await?;
    let can_manage = if let Some((_,token)) = &session {
        state.api.get(&format!("/v1/addons/{}/permissions",segment(&slug)),Some(token)).await
            .map(|p|p["owner"]==true || p["admin"]==true || p["edit_description"]==true).unwrap_or(false)
    } else {false};
    let comments = i["comments"].as_array().into_iter().flatten().map(|c| IssueComment {
        author:s(&c["author"]), body:s(&c["body"])
    }).collect();
    Ok(render(&IssuePage { user, csrf:csrf.0, slug, name:s(&a["name"]), id,
        title:s(&i["title"]), body:s(&i["body"]), kind:s(&i["kind"]), status:s(&i["status"]),
        author:s(&i["author"]), comments, can_manage }))
}
pub async fn comment(State(state): State<AppState>, jar: PrivateCookieJar,
    Path((slug,id)): Path<(String,i64)>, Form(form): Form<CommentForm>) -> WebResult<Redirect> {
    let (_,token) = visitor(&state,&jar).await?.ok_or(WebError::LoginRequired)?;
    state.api.post(&format!("/v1/addons/{}/issues/{id}/comments",segment(&slug)),Some(&token),
        &json!({"body":form.body})).await?;
    Ok(Redirect::to(&format!("/addons/{}?tab=issues&issue={id}#issue-detail",segment(&slug))))
}
pub async fn status(State(state): State<AppState>, jar: PrivateCookieJar,
    Path((slug,id)): Path<(String,i64)>, Form(form): Form<StatusForm>) -> WebResult<Redirect> {
    let (_,token) = visitor(&state,&jar).await?.ok_or(WebError::LoginRequired)?;
    state.api.call(reqwest::Method::PATCH,
        &format!("/v1/addons/{}/issues/{id}",segment(&slug)),Some(&token),
        Some(&json!({"status":form.status}))).await?;
    Ok(Redirect::to(&format!("/addons/{}?tab=issues&issue={id}#issue-detail",segment(&slug))))
}
