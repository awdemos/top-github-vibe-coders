use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde_json::json;
use std::sync::Arc;

use crate::{auth::{get_current_user_from_headers, get_csrf_token_from_headers}, db, AppState};

pub async fn toggle_vote(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(nominee_id): Path<i64>,
) -> Result<impl IntoResponse, StatusCode> {
    let user = get_current_user_from_headers(&headers, &state.session_manager)
        .ok_or(StatusCode::UNAUTHORIZED)?;

    // Validate CSRF token
    let csrf_header = get_csrf_token_from_headers(&headers).ok_or(StatusCode::FORBIDDEN)?;
    let cookie_header = headers
        .get(axum::http::header::COOKIE)
        .and_then(|h| h.to_str().ok())
        .ok_or(StatusCode::FORBIDDEN)?;
    let session_value = crate::auth::parse_cookie(cookie_header, crate::auth::SESSION_COOKIE_NAME)
        .ok_or(StatusCode::FORBIDDEN)?;
    if !state.session_manager.verify_csrf(session_value, &csrf_header) {
        return Err(StatusCode::FORBIDDEN);
    }

    // Validate nominee exists and is approved
    let nominee = db::get_nominee_by_id(&state.db, nominee_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to fetch nominee: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    match nominee {
        None => return Err(StatusCode::NOT_FOUND),
        Some(n) if n.nomination_status != "approved" => return Err(StatusCode::NOT_FOUND),
        _ => {}
    }

    let vote_type = if user.is_claw_bot {
        "claw_bot"
    } else {
        "community"
    };

    let already_voted = db::get_user_voted(&state.db, user.id, nominee_id)
        .await
        .map_err(|e| {
            tracing::error!("Vote check failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let voted = if user.is_claw_bot {
        already_voted.1
    } else {
        already_voted.0
    };

    if voted {
        db::remove_vote(&state.db, user.id, nominee_id, vote_type)
            .await
            .map_err(|e| {
                tracing::error!("Remove vote failed: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
    } else {
        db::cast_vote(&state.db, user.id, nominee_id, vote_type)
            .await
            .map_err(|e| {
                tracing::error!("Cast vote failed: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
    }

    let (community, claw) = db::get_vote_counts(&state.db, nominee_id)
        .await
        .map_err(|e| {
            tracing::error!("Vote count failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(json!({
        "community": community,
        "claw_bot": claw,
        "voted": !voted,
    })))
}
