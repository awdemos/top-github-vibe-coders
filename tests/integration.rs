use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::{get, post};
use std::sync::Arc;
use tower::ServiceExt;

use top_github_vibe_coders::{
    auth, config, db, github, health, leaderboard, nominations, rate_limit, voting, AppState,
};

async fn setup_app() -> axum::Router {
    let path = format!(
        "/tmp/vibe_coders_integration_{}.db",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let _ = std::fs::remove_file(&path);
    let db = libsql::Builder::new_local(&path)
        .build()
        .await
        .unwrap()
        .connect()
        .unwrap();
    db::init_db(&db).await.unwrap();

    let cfg = config::Config {
        database_url: path.clone(),
        turso_auth_token: "test".to_string(),
        github_client_id: "test".to_string(),
        github_client_secret: "test".to_string(),
        github_token: "test".to_string(),
        session_secret: "a-very-secret-test-key-123456789012".to_string(),
        app_url: "http://localhost:3000".to_string(),
    };

    let github_client = github::GitHubClient::with_base_url("test".to_string(), "http://localhost:9999".to_string());
    let session_manager = auth::SessionManager::new(cfg.session_secret.clone());

    let state = Arc::new(AppState {
        config: cfg,
        db,
        github_client,
        session_manager,
        auth_limiter: rate_limit::build_limiter(100, 100),
        vote_limiter: rate_limit::build_limiter(100, 100),
        general_limiter: rate_limit::build_limiter(100, 100),
    });

    axum::Router::new()
        .route("/", get(leaderboard::leaderboard))
        .route("/health", get(health::health_check))
        .route("/nominate", get(nominations::nominate_page).post(nominations::submit_nomination))
        .route("/vote/:nominee_id", post(voting::toggle_vote))
        .route("/auth/github", get(auth::github_login))
        .route("/auth/github/callback", get(auth::github_callback))
        .route("/logout", post(auth::logout))
        .with_state(state)
}

#[tokio::test]
async fn test_health_ok() {
    let app = setup_app().await;
    let response = app
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_homepage_ok() {
    let app = setup_app().await;
    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_nominate_page_ok() {
    let app = setup_app().await;
    let response = app
        .oneshot(Request::builder().uri("/nominate").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_vote_without_auth_returns_401() {
    let app = setup_app().await;
    let response = app
        .oneshot(
            Request::builder()
                .uri("/vote/1")
                .method("POST")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_logout_without_session_redirects() {
    let app = setup_app().await;
    let response = app
        .oneshot(
            Request::builder()
                .uri("/logout")
                .method("POST")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn test_auth_callback_without_code_returns_400() {
    let app = setup_app().await;
    let response = app
        .oneshot(
            Request::builder()
                .uri("/auth/github/callback?state=foo")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_nominate_post_without_auth_returns_401() {
    let app = setup_app().await;
    let response = app
        .oneshot(
            Request::builder()
                .uri("/nominate")
                .method("POST")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("github_login=testuser"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
