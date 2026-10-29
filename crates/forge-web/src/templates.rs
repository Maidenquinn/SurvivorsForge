//! Askama templates. Every variable is HTML-escaped; the only exception is an addon's
//! `description_html`, which the API sanitized.

use askama::Template;
use axum::response::Html;
use serde_json::Value;

/// The logged-in visitor as the API describes them (`/v1/me`).
#[derive(Debug, Clone, Default)]
pub struct Visitor {
    pub login: String,
    pub avatar_url: String,
    pub admin: bool,
}

impl Visitor {
    pub fn from_me(me: &Value) -> Visitor {
        Visitor {
            login: me["login"].as_str().unwrap_or_default().to_string(),
            avatar_url: me["avatar_url"].as_str().unwrap_or_default().to_string(),
            admin: me["role"] == "admin",
        }
    }
}

pub fn render(template: &impl Template) -> Html<String> {
    Html(template.render().unwrap_or_else(|e| {
        tracing::error!(error = %e, "template failed");
        "Internal error".into()
    }))
}

#[derive(Template)]
#[template(path = "error.html")]
pub struct ErrorPage {
    pub title: String,
    pub message: String,
    pub user: Option<Visitor>,
    pub csrf: String,
}

#[derive(Template)]
#[template(path = "index.html")]
pub struct IndexPage {
    pub user: Option<Visitor>,
    pub csrf: String,
}
