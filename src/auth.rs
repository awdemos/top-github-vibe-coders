use axum::{
    extract::{Query, State},
    response::{IntoResponse, Redirect},
    http::{HeaderMap, StatusCode},
};
use cookie::{Cookie, SameSite};
use hmac::{Hmac, Mac};
use oauth2::{basic::BasicClient, AuthUrl, ClientId, ClientSecret, CsrfToken, RedirectUrl, TokenUrl};
use rand::{distributions::Alphanumeric, Rng};
use serde::Deserialize;
use sha2::Sha256;
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use crate::{config::Config, db, models::User};

pub const SESSION_COOKIE_NAME: &str = "session";
pub const CSRF_COOKIE_NAME: &str = "csrf_token";

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone)]
pub struct SessionManager {
    secret: String,
    sessions: Arc<RwLock<HashMap<String, User>>>,
    csrf_tokens: Arc<RwLock<HashMap<String, String>>>,
}

impl SessionManager {
    pub fn new(secret: String) -> Self {
        assert!(!secret.is_empty(), "SESSION_SECRET must not be empty");
        Self {
            secret,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            csrf_tokens: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn sign(&self, session_id: &str) -> String {
        let mut mac = HmacSha256::new_from_slice(self.secret.as_bytes())
            .expect("SESSION_SECRET was validated non-empty at startup");
        mac.update(session_id.as_bytes());
        hex::encode(mac.finalize().into_bytes())
    }

    pub fn create_session(&self, user: &User) -> (String, String) {
        let session_id: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(32)
            .map(char::from)
            .collect();

        let csrf_token: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(32)
            .map(char::from)
            .collect();

        let signature = self.sign(&session_id);
        let cookie_value = format!("{signature}.{session_id}");

        self.sessions
            .write()
            .unwrap()
            .insert(session_id.clone(), user.clone());
        self.csrf_tokens
            .write()
            .unwrap()
            .insert(session_id.clone(), csrf_token.clone());

        let session_cookie = Cookie::build((SESSION_COOKIE_NAME, cookie_value))
            .http_only(true)
            .same_site(SameSite::Lax)
            .path("/")
            .build()
            .to_string();

        let csrf_cookie = Cookie::build((CSRF_COOKIE_NAME, csrf_token))
            .same_site(SameSite::Lax)
            .path("/")
            .build()
            .to_string();

        (session_cookie, csrf_cookie)
    }

    pub fn verify_session(&self, cookie_value: &str) -> Option<User> {
        let parts: Vec<&str> = cookie_value.splitn(2, '.').collect();
        if parts.len() != 2 {
            return None;
        }

        let (sig, session_id) = (parts[0], parts[1]);
        let expected_sig = self.sign(session_id);

        if sig != expected_sig {
            return None;
        }

        self.sessions.read().unwrap().get(session_id).cloned()
    }

    pub fn verify_csrf(&self, session_cookie_value: &str, csrf_token: &str) -> bool {
        let parts: Vec<&str> = session_cookie_value.splitn(2, '.').collect();
        if parts.len() != 2 {
            return false;
        }

        let (_, session_id) = (parts[0], parts[1]);
        self.csrf_tokens
            .read()
            .unwrap()
            .get(session_id)
            .map(|t| t == csrf_token)
            .unwrap_or(false)
    }

    pub fn destroy_session(&self, cookie_value: &str) {
        let parts: Vec<&str> = cookie_value.splitn(2, '.').collect();
        if let Some(session_id) = parts.get(1) {
            self.sessions.write().unwrap().remove(*session_id);
            self.csrf_tokens.write().unwrap().remove(*session_id);
        }
    }
}

pub fn build_oauth_client(config: &Config) -> BasicClient {
    BasicClient::new(
        ClientId::new(config.github_client_id.clone()),
        Some(ClientSecret::new(config.github_client_secret.clone())),
        AuthUrl::new("https://github.com/login/oauth/authorize".to_string()).unwrap(),
        Some(TokenUrl::new("https://github.com/login/oauth/access_token".to_string()).unwrap()),
    )
    .set_redirect_uri(
        RedirectUrl::new(format!("{}/auth/github/callback", config.app_url)).unwrap(),
    )
}

#[derive(Deserialize)]
pub struct AuthQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
}

pub fn parse_cookie<'a>(cookies: &'a str, name: &str) -> Option<&'a str> {
    for cookie in cookies.split(';') {
        let cookie = cookie.trim();
        if let Some(value) = cookie.strip_prefix(&format!("{name}=")) {
            return Some(value);
        }
    }
    None
}

pub fn get_current_user_from_headers(
    headers: &HeaderMap,
    session_manager: &SessionManager,
) -> Option<User> {
    let cookie_header = headers.get(axum::http::header::COOKIE)?;
    let cookies = cookie_header.to_str().ok()?;
    let session_value = parse_cookie(cookies, SESSION_COOKIE_NAME)?;
    session_manager.verify_session(session_value)
}

pub fn get_csrf_token_from_headers(headers: &HeaderMap) -> Option<String> {
    headers
        .get("X-CSRF-Token")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

pub fn is_secure_context(config: &Config) -> bool {
    config.app_url.starts_with("https://")
}

pub async fn github_login(State(state): State<Arc<crate::AppState>>) -> impl IntoResponse {
    let client = build_oauth_client(&state.config);
    let (auth_url, csrf_token) = client.authorize_url(CsrfToken::new_random).url();

    let mut response = Redirect::temporary(auth_url.as_str()).into_response();
    let secure = is_secure_context(&state.config);
    let state_cookie = Cookie::build(("oauth_state", csrf_token.secret().clone()))
        .http_only(true)
        .same_site(SameSite::Lax)
        .path("/")
        .secure(secure)
        .build()
        .to_string();

    response.headers_mut().insert(
        axum::http::header::SET_COOKIE,
        state_cookie.parse().unwrap(),
    );

    response
}

pub async fn github_callback(
    State(state): State<Arc<crate::AppState>>,
    headers: HeaderMap,
    Query(params): Query<AuthQuery>,
) -> Result<impl IntoResponse, StatusCode> {
    // Handle OAuth errors (user denied access, etc.)
    if let Some(error) = params.error {
        tracing::error!("OAuth error: {} - {}", error, params.error_description.unwrap_or_default());
        return Err(StatusCode::BAD_REQUEST);
    }

    let code = params.code.ok_or(StatusCode::BAD_REQUEST)?;
    let oauth_state = params.state.ok_or(StatusCode::BAD_REQUEST)?;

    // Verify state parameter
    let cookie_header = headers
        .get(axum::http::header::COOKIE)
        .and_then(|h| h.to_str().ok())
        .ok_or(StatusCode::BAD_REQUEST)?;

    let mut state_valid = false;
    for cookie in cookie_header.split(';') {
        let cookie = cookie.trim();
        if let Some(value) = cookie.strip_prefix("oauth_state=") {
            if value == oauth_state {
                state_valid = true;
            }
        }
    }

    if !state_valid {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Exchange code for access token
    let token_resp = state
        .github_client
        .client
        .post("https://github.com/login/oauth/access_token")
        .header("Accept", "application/json")
        .form(&[
            ("client_id", state.config.github_client_id.as_str()),
            ("client_secret", state.config.github_client_secret.as_str()),
            ("code", &code),
        ])
        .send()
        .await
        .map_err(|e| {
            tracing::error!("Token exchange failed: {}", e);
            StatusCode::BAD_REQUEST
        })?
        .json::<serde_json::Value>()
        .await
        .map_err(|e| {
            tracing::error!("Token parse failed: {}", e);
            StatusCode::BAD_REQUEST
        })?;

    let access_token = token_resp["access_token"]
        .as_str()
        .ok_or(StatusCode::BAD_REQUEST)?;

    // Fetch authenticated user
    let github_user = state
        .github_client
        .get_authenticated_user(access_token)
        .await
        .map_err(|e| {
            tracing::error!("User fetch failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Check if claw bot
    let is_claw = state
        .github_client
        .is_likely_claw_bot(&github_user.login)
        .await
        .unwrap_or(false);

    // Save to DB
    let user = db::get_or_create_user(
        &state.db,
        github_user.id,
        &github_user.login,
        Some(&github_user.avatar_url),
        is_claw,
    )
    .await
    .map_err(|e| {
        tracing::error!("DB error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Create session
    let (session_cookie, csrf_cookie) = state.session_manager.create_session(&user);
    let secure = is_secure_context(&state.config);

    let mut response = Redirect::to("/").into_response();
    let session_cookie = Cookie::build((SESSION_COOKIE_NAME, session_cookie))
        .http_only(true)
        .same_site(SameSite::Lax)
        .path("/")
        .secure(secure)
        .build()
        .to_string();
    response.headers_mut().insert(
        axum::http::header::SET_COOKIE,
        session_cookie.parse().unwrap(),
    );

    let csrf_cookie = Cookie::build((CSRF_COOKIE_NAME, csrf_cookie))
        .same_site(SameSite::Lax)
        .path("/")
        .secure(secure)
        .build()
        .to_string();
    response.headers_mut().append(
        axum::http::header::SET_COOKIE,
        csrf_cookie.parse().unwrap(),
    );

    // Clear oauth state cookie
    let clear_state = Cookie::build(("oauth_state", ""))
        .http_only(true)
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(cookie::time::Duration::seconds(0))
        .secure(secure)
        .build()
        .to_string();

    response.headers_mut().append(
        axum::http::header::SET_COOKIE,
        clear_state.parse().unwrap(),
    );

    Ok(response)
}

pub async fn logout(
    State(state): State<Arc<crate::AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let secure = is_secure_context(&state.config);

    if let Some(cookie_header) = headers.get(axum::http::header::COOKIE) {
        if let Ok(cookies) = cookie_header.to_str() {
            for cookie in cookies.split(';') {
                let cookie = cookie.trim();
                if let Some(value) = cookie.strip_prefix(&format!("{SESSION_COOKIE_NAME}=")) {
                    state.session_manager.destroy_session(value);
                }
            }
        }
    }

    let mut response = Redirect::to("/").into_response();

    let clear_session = Cookie::build((SESSION_COOKIE_NAME, ""))
        .http_only(true)
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(cookie::time::Duration::seconds(0))
        .secure(secure)
        .build()
        .to_string();
    response.headers_mut().insert(
        axum::http::header::SET_COOKIE,
        clear_session.parse().unwrap(),
    );

    let clear_csrf = Cookie::build((CSRF_COOKIE_NAME, ""))
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(cookie::time::Duration::seconds(0))
        .secure(secure)
        .build()
        .to_string();
    response.headers_mut().append(
        axum::http::header::SET_COOKIE,
        clear_csrf.parse().unwrap(),
    );

    response
}
