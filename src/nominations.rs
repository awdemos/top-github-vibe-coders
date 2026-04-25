use askama::Template;
use axum::{extract::State, http::HeaderMap, response::IntoResponse};
use std::sync::Arc;

use crate::{auth::{get_current_user_from_headers, get_csrf_token_from_headers}, AppState};

#[derive(Template)]
#[template(path = "nominate.html")]
pub struct NominateTemplate {
    pub user: Option<crate::models::User>,
    pub csrf_token: Option<String>,
}

pub async fn nominate_page(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let user = get_current_user_from_headers(&headers, &state.session_manager);
    crate::HtmlTemplate(NominateTemplate {
        user,
        csrf_token: get_csrf_token_from_headers(&headers),
    })
}
