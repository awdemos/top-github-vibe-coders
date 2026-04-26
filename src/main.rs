use axum::{
    middleware,
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use tower_http::{
    catch_panic::CatchPanicLayer,
    compression::CompressionLayer,
    services::ServeDir,
    timeout::TimeoutLayer,
    trace::TraceLayer,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Layer};

use top_github_vibe_coders::{
    auth, config, db, github, health, leaderboard, nominations, rate_limit, request_id, voting,
    AppState,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Determine log format from env: json or pretty
    let log_json = std::env::var("RUST_LOG_JSON").unwrap_or_default() == "true";

    let fmt_layer = if log_json {
        tracing_subscriber::fmt::layer().json().boxed()
    } else {
        tracing_subscriber::fmt::layer().boxed()
    };

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "top_github_vibe_coders=info,tower_http=info".into()),
        )
        .with(fmt_layer)
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
        .route("/health", get(health::health_check))
        .route("/nominate", get(nominations::nominate_page).post(nominations::submit_nomination))
        .route("/logout", post(auth::logout))
        .nest_service("/static", ServeDir::new("static"))
        // Request ID tracing (innermost — runs first on request, last on response)
        .layer(middleware::from_fn(request_id::request_id_middleware))
        .layer(middleware::from_fn_with_state(state.clone(), rate_limit::general_limit_middleware))
        .layer(TraceLayer::new_for_http())
        .layer(CatchPanicLayer::new())
        .layer(TimeoutLayer::new(std::time::Duration::from_secs(30)))
        .layer(CompressionLayer::new())
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

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!("Received Ctrl+C, shutting down gracefully..."),
        _ = terminate => tracing::info!("Received SIGTERM, shutting down gracefully..."),
    }
}
