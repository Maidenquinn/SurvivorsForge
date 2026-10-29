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
const PAGE_SIZE: usize = 30;
fn owns_comment(user_id: i64, author_id: Option<i64>) -> bool { user_id > 0 && author_id == Some(user_id) }
fn next_cursor(data: &[Value]) -> Option<i64> {
    if data.len() <= PAGE_SIZE { None } else { data.get(PAGE_SIZE - 1).and_then(|c| c["id"].as_i64()) }
}
#[derive(Deserialize)]
pub struct Page { before: Option<i64> }
#[derive(Deserialize)]
pub struct Body { body: String }
#[derive(Clone)]
pub struct Comment { pub id: i64, pub author: String, pub avatar: String, pub body: String, pub created: String, pub edited: bool, pub can_edit: bool, pub can_delete: bool }
#[derive(Template)]
#[template(path="comments.html")]
struct CommentsPage { user: Option<crate::templates::Visitor>, csrf: String, slug: String, name: String, comments: Vec<Comment>, next: Option<i64> }

pub async fn feed(state: &AppState, slug: &str, session: &Option<(crate::templates::Visitor, String)>, before: Option<i64>) -> WebResult<(Vec<Comment>, Option<i64>, bool)> {
    let user = session.as_ref().map(|(v,_)| v.clone());
    let path = format!("/v1/addons/{}/comments{}",seg(slug),before.map(|n|format!("?before={n}")).unwrap_or_default());
    let data = state.api.get(&path,None).await?;
    let perms = if let Some((_,token)) = &session {
        state.api.get(&format!("/v1/addons/{}/permissions",seg(&slug)),Some(token)).await.ok()
    } else {None};
    let moderator = user.as_ref().is_some_and(|u|u.admin) || perms.as_ref().is_some_and(|p|p["owner"]==true || p["admin"]==true || p["edit_description"]==true);
    let next = data.as_array().and_then(|items| next_cursor(items));
    let comments: Vec<Comment> = data.as_array().into_iter().flatten().take(PAGE_SIZE).map(|c| {
        let author=strval(&c["author"]);
        let own=user.as_ref().is_some_and(|u|owns_comment(u.id, c["author_id"].as_i64()));
        Comment { id:c["id"].as_i64().unwrap_or_default(), author, avatar:strval(&c["avatar_url"]), body:strval(&c["body"]),
            created:strval(&c["created_at"]).chars().take(10).collect(), edited:c["edited_at"].is_string(),can_edit:own || user.as_ref().is_some_and(|u|u.admin),can_delete:own || moderator }
    }).collect();
    Ok((comments,next,moderator))
}

pub async fn list(State(state): State<AppState>, jar: PrivateCookieJar, Extension(csrf): Extension<CsrfToken>,
    Path(slug): Path<String>, Query(page): Query<Page>) -> WebResult<Html<String>> {
    let session = visitor(&state, &jar).await?;
    let user = session.as_ref().map(|(v,_)| v.clone());
    let addon = state.api.get(&format!("/v1/addons/{}",seg(&slug)),None).await?;
    if addon["blocked"] == true {return Err(WebError::NotFound);}
    let (comments,next,_) = feed(&state, &slug, &session, page.before).await?;
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

#[cfg(test)]
mod tests {
    use super::{owns_comment, next_cursor};
    use serde_json::json;

    #[test]
    fn identity_uses_numeric_user_id_not_display_name() {
        assert!(owns_comment(12, Some(12)));
        assert!(!owns_comment(12, Some(13)));
        assert!(!owns_comment(0, Some(0)));
        assert!(!owns_comment(12, None));
    }

    #[test]
    fn pagination_only_exposes_next_when_another_row_exists() {
        let rows: Vec<_> = (1..=31).rev().map(|id| json!({"id": id})).collect();
        assert_eq!(next_cursor(&rows), Some(2));
        assert_eq!(next_cursor(&rows[..30]), None);
        assert_eq!(next_cursor(&rows[..1]), None);
        assert_eq!(next_cursor(&[]), None);
    }
}
