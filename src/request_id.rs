use axum::{
    extract::Request,
    http::HeaderValue,
    middleware::Next,
    response::Response,
};
use uuid::Uuid;

const REQUEST_ID_HEADER: &str = "x-request-id";

pub async fn request_id_middleware(mut request: Request, next: Next) -> Response {
    let request_id = request
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|h| h.to_str().ok())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    // Set the request ID on the request for handlers to access
    request.headers_mut().insert(
        REQUEST_ID_HEADER,
        HeaderValue::from_str(&request_id).unwrap(),
    );

    // Create a tracing span with the request ID
    let span = tracing::info_span!("request", request_id = %request_id);
    let response = next.run(request).instrument(span).await;

    response
}

use tracing::Instrument;
