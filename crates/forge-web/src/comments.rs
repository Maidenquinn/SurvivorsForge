//! Public addon comment feed. All mutations go through the authenticated API.
use crate::app::{AppState, visitor};
use crate::csrf::CsrfToken;
use crate::error::{WebError, WebResult};
use axum::{Extension, extract::{Path, Form, Query, State}, response::{Html, Redirect}};
use axum_extra::extract::PrivateCookieJar;
use askama::Template;
use serde::Deserialize;
use serde_json::{Value, json};

fn seg(s: &str) -> String { s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {(b as char).to_string()} else {format!("%{b:02X}")}).collect() }
fn strval(v: &Value) -> String { v.as_str().unwrap_or_default().to_string() }
#[derive(Deserialize)]
pub struct Page { before: Option<i64> }
#[derive(Deserialize)]
pub struct Body { body: String }
#[derive(Clone)]
struct Comment { id: i64, author: String, avatar: String, body: String, created: String, edited: bool, can_edit: bool, can_delete: bool }
#[derive(Template)]
#[template(path="comments.html")]
struct CommentsPage { user: Option<crate::templates::Visitor>, csrf: String, slug: String, name: String, comments: Vec<Comment>, next: Option<i64> }

pub async fn list(State(state): State<AppState>, jar: PrivateCookieJar, Extension(csrf): Extension<CsrfToken>,
    Path(slug): Path<String>, Query(page): Query<Page>) -> WebResult<Html<String>> {
    let session = visitor(&state, &jar).await?;
    let user = session.as_ref().map(|(v,_)| v.clone());
    let addon = state.api.get(&format!("/v1/addons/{}",seg(&slug)),None).await?;
    if addon["blocked"] == true {return Err(WebError::NotFound);}
    let path = format!("/v1/addons/{}/comments{}",seg(&slug),page.before.map(|n|format!("?before={n}")).unwrap_or_default());
    let data = state.api.get(&path,None).await?;
    let perms = if let Some((_,token)) = &session {
        state.api.get(&format!("/v1/addons/{}/permissions",seg(&slug)),Some(token)).await.ok()
    } else {None};
    let moderator = user.as_ref().is_some_and(|u|u.admin) || perms.as_ref().is_some_and(|p|p["owner"]==true || p["admin"]==true || p["edit_description"]==true);
    let has_more = data.as_array().is_some_and(|items| items.len() > 30);
    let comments: Vec<Comment> = data.as_array().into_iter().flatten().take(30).map(|c| {
        let author=strval(&c["author"]);
        let own=user.as_ref().is_some_and(|u|u.id > 0 && c["author_id"].as_i64() == Some(u.id));
        Comment { id:c["id"].as_i64().unwrap_or_default(), author, avatar:strval(&c["avatar_url"]), body:strval(&c["body"]),
            created:strval(&c["created_at"]).chars().take(10).collect(), edited:c["edited_at"].is_string(),can_edit:own || user.as_ref().is_some_and(|u|u.admin),can_delete:own || moderator }
    }).collect();
    let next=if has_more {comments.last().map(|c|c.id)} else {None};
    Ok(Html(CommentsPage {user,csrf:csrf.0,slug,name:strval(&addon["name"]),comments,next}.render().map_err(|_|WebError::NotFound)?))
}
pub async fn create(State(state): State<AppState>,jar:PrivateCookieJar,Path(slug):Path<String>,Form(body):Form<Body>)->WebResult<Redirect>{
    let (_,token)=visitor(&state,&jar).await?.ok_or(WebError::LoginRequired)?;
    state.api.post(&format!("/v1/addons/{}/comments",seg(&slug)),Some(&token),&json!({"body":body.body})).await?;
    Ok(Redirect::to(&format!("/addons/{}/comments",seg(&slug))))
}
pub async fn edit(State(state): State<AppState>,jar:PrivateCookieJar,Path((slug,id)):Path<(String,i64)>,Form(body):Form<Body>)->WebResult<Redirect>{
    let (_,token)=visitor(&state,&jar).await?.ok_or(WebError::LoginRequired)?;
    state.api.call(reqwest::Method::PATCH,&format!("/v1/addons/{}/comments/{id}",seg(&slug)),Some(&token),Some(&json!({"body":body.body}))).await?;
    Ok(Redirect::to(&format!("/addons/{}/comments",seg(&slug))))
}
pub async fn delete(State(state): State<AppState>,jar:PrivateCookieJar,Path((slug,id)):Path<(String,i64)>)->WebResult<Redirect>{
    let (_,token)=visitor(&state,&jar).await?.ok_or(WebError::LoginRequired)?;
    state.api.call(reqwest::Method::DELETE,&format!("/v1/addons/{}/comments/{id}",seg(&slug)),Some(&token),None).await?;
    Ok(Redirect::to(&format!("/addons/{}/comments",seg(&slug))))
}
