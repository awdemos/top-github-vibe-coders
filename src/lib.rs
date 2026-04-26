pub mod auth;
pub mod config;
pub mod db;
pub mod github;
pub mod health;
pub mod leaderboard;
pub mod models;
pub mod nominations;
pub mod rate_limit;
pub mod request_id;
pub mod voting;

use axum::{
    response::{Html, IntoResponse, Response},
    http::StatusCode,
};
use askama::Template;

pub struct HtmlTemplate<T>(pub T);

impl<T: Template> IntoResponse for HtmlTemplate<T> {
    fn into_response(self) -> Response {
        match self.0.render() {
            Ok(html) => Html(html).into_response(),
            Err(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to render template: {err}"),
            )
                .into_response(),
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub config: config::Config,
    pub db: libsql::Connection,
    pub github_client: github::GitHubClient,
    pub session_manager: auth::SessionManager,
    pub auth_limiter: rate_limit::KeyedRateLimiter,
    pub vote_limiter: rate_limit::KeyedRateLimiter,
    pub general_limiter: rate_limit::KeyedRateLimiter,
}
