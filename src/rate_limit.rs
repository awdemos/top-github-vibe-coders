use axum::{
    extract::{ConnectInfo, Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use governor::{clock::DefaultClock, state::keyed::DefaultKeyedStateStore, Quota, RateLimiter};
use std::net::SocketAddr;
use std::num::NonZeroU32;
use std::sync::Arc;

use crate::AppState;

pub type KeyedRateLimiter = Arc<RateLimiter<String, DefaultKeyedStateStore<String>, DefaultClock>>;

pub fn build_limiter(per_second: u32, burst: u32) -> KeyedRateLimiter {
    Arc::new(RateLimiter::keyed(
        Quota::per_second(NonZeroU32::new(per_second).unwrap())
            .allow_burst(NonZeroU32::new(burst).unwrap()),
    ))
}

fn client_ip(req: &Request) -> String {
    // Try X-Forwarded-For first (for proxies)
    if let Some(header) = req.headers().get("x-forwarded-for") {
        if let Ok(val) = header.to_str() {
            return val.split(',').next().unwrap_or(val).trim().to_string();
        }
    }
    // Fallback to ConnectInfo
    if let Some(ConnectInfo(addr)) = req.extensions().get::<ConnectInfo<SocketAddr>>() {
        return addr.ip().to_string();
    }
    "unknown".to_string()
}

pub async fn auth_limit_middleware(
    State(state): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let ip = client_ip(&req);
    if state.auth_limiter.check_key(&ip).is_err() {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }
    Ok(next.run(req).await)
}

pub async fn vote_limit_middleware(
    State(state): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let ip = client_ip(&req);
    if state.vote_limiter.check_key(&ip).is_err() {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }
    Ok(next.run(req).await)
}

pub async fn general_limit_middleware(
    State(state): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let ip = client_ip(&req);
    if state.general_limiter.check_key(&ip).is_err() {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }
    Ok(next.run(req).await)
}
