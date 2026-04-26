use axum::{
    middleware,
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use tower_http::{services::ServeDir, trace::TraceLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use top_github_vibe_coders::{
    auth, config, db, github, leaderboard, nominations, rate_limit, voting, AppState,
};

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
        auth_limiter: rate_limit::build_limiter(1, 5),    // ~5/min burst
        vote_limiter: rate_limit::build_limiter(2, 30),   // ~120/min burst
        general_limiter: rate_limit::build_limiter(4, 60), // ~240/min burst
    });

    let app = Router::new()
        .route("/", get(leaderboard::leaderboard))
        .route("/nominate", get(nominations::nominate_page).post(nominations::submit_nomination))
        .route("/logout", post(auth::logout))
        .nest_service("/static", ServeDir::new("static"))
        .layer(middleware::from_fn_with_state(state.clone(), rate_limit::general_limit_middleware))
        .layer(TraceLayer::new_for_http())
        .merge(
            Router::new()
                .route("/auth/github", get(auth::github_login))
                .route("/auth/github/callback", get(auth::github_callback))
                .layer(middleware::from_fn_with_state(state.clone(), rate_limit::auth_limit_middleware))
                .with_state(state.clone()),
        )
        .merge(
            Router::new()
                .route("/vote/:nominee_id", post(voting::toggle_vote))
                .layer(middleware::from_fn_with_state(state.clone(), rate_limit::vote_limit_middleware))
                .with_state(state.clone()),
        )
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    tracing::info!("Listening on {}", listener.local_addr()?);
    axum::serve(listener, app).await?;

    Ok(())
}
