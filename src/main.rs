mod auth;
mod config;
mod db;
mod github;
mod leaderboard;
mod models;
mod nominations;
mod voting;

use std::sync::Arc;
use axum::{
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    http::StatusCode,
    Router,
};
use askama::Template;
use tower_http::{services::ServeDir, trace::TraceLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

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
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "top_github_vibe_coders=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = config::Config::from_env()?;

    tracing::info!("Connecting to Turso database...");
    let db = libsql::Builder::new_remote(config.database_url.clone(), config.turso_auth_token.clone())
        .build()
        .await?
        .connect()?;

    db::init_db(&db).await?;
    tracing::info!("Database initialized");

    let github_client = github::GitHubClient::new(config.github_token.clone());
    let session_manager = auth::SessionManager::new(config.session_secret.clone());

    let state = Arc::new(AppState {
        config,
        db,
        github_client,
        session_manager,
    });

    let app = Router::new()
        .route("/", get(leaderboard::leaderboard))
        .route("/nominate", get(nominations::nominate_page))
        .route("/vote/:nominee_id", post(voting::toggle_vote))
        .route("/auth/github", get(auth::github_login))
        .route("/auth/github/callback", get(auth::github_callback))
        .route("/logout", post(auth::logout))
        .nest_service("/static", ServeDir::new("static"))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    tracing::info!("Listening on {}", listener.local_addr()?);
    axum::serve(listener, app).await?;

    Ok(())
}
