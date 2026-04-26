use askama::Template;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Form,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::{
    auth::{get_current_user_from_headers, get_csrf_token_from_headers},
    db, AppState,
};

#[derive(Template)]
#[template(path = "nominate.html")]
pub struct NominateTemplate {
    pub user: Option<crate::models::User>,
    pub csrf_token: Option<String>,
    pub error: Option<String>,
    pub success: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct NominationForm {
    pub github_login: String,
    pub evidence_url: Option<String>,
    pub reason: Option<String>,
}

pub async fn nominate_page(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let user = get_current_user_from_headers(&headers, &state.session_manager);
    crate::HtmlTemplate(NominateTemplate {
        user,
        csrf_token: get_csrf_token_from_headers(&headers),
        error: None,
        success: None,
    })
}

pub async fn submit_nomination(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Form(form): Form<NominationForm>,
) -> Result<impl IntoResponse, StatusCode> {
    let user = get_current_user_from_headers(&headers, &state.session_manager)
        .ok_or(StatusCode::UNAUTHORIZED)?;

    // Validate CSRF token
    let csrf_header = crate::auth::get_csrf_token_from_headers(&headers).ok_or(StatusCode::FORBIDDEN)?;
    let cookie_header = headers
        .get(axum::http::header::COOKIE)
        .and_then(|h| h.to_str().ok())
        .ok_or(StatusCode::FORBIDDEN)?;
    let session_value = crate::auth::parse_cookie(cookie_header, crate::auth::SESSION_COOKIE_NAME)
        .ok_or(StatusCode::FORBIDDEN)?;
    if !state.session_manager.verify_csrf(session_value, &csrf_header) {
        return Err(StatusCode::FORBIDDEN);
    }

    let login = form.github_login.trim();
    if login.is_empty() || login.len() > 39 {
        return Ok(crate::HtmlTemplate(NominateTemplate {
            user: Some(user),
            csrf_token: get_csrf_token_from_headers(&headers),
            error: Some("Invalid GitHub username".to_string()),
            success: None,
        }));
    }

    // Validate username format (alphanumeric + hyphens, no leading hyphen)
    if !login.chars().all(|c| c.is_alphanumeric() || c == '-')
        || login.starts_with('-')
        || login.starts_with("--")
    {
        return Ok(crate::HtmlTemplate(NominateTemplate {
            user: Some(user),
            csrf_token: get_csrf_token_from_headers(&headers),
            error: Some("GitHub username contains invalid characters".to_string()),
            success: None,
        }));
    }

    // Check if already nominated
    match db::get_nominee_by_login(&state.db, login).await {
        Ok(Some(_)) => {
            return Ok(crate::HtmlTemplate(NominateTemplate {
                user: Some(user),
                csrf_token: get_csrf_token_from_headers(&headers),
                error: Some("That user has already been nominated".to_string()),
                success: None,
            }));
        }
        Ok(None) => {}
        Err(e) => {
            tracing::error!("DB error checking nominee: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    }

    // Verify GitHub user exists
    match state.github_client.get_user(login).await {
        Ok(gh_user) => {
            if let Err(e) = db::insert_nominee(&state.db, &gh_user.login, Some(&gh_user.avatar_url)).await {
                tracing::error!("Failed to insert nominee: {}", e);
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
        Err(e) => {
            tracing::warn!("GitHub user not found: {}", e);
            return Ok(crate::HtmlTemplate(NominateTemplate {
                user: Some(user),
                csrf_token: get_csrf_token_from_headers(&headers),
                error: Some("GitHub user not found".to_string()),
                success: None,
            }));
        }
    }

    Ok(crate::HtmlTemplate(NominateTemplate {
        user: Some(user),
        csrf_token: get_csrf_token_from_headers(&headers),
        error: None,
        success: Some(format!("Nominated {login}! They'll be reviewed by the pipeline.")),
    }))
}
