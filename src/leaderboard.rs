use askama::Template;
use axum::{
    extract::{Query, State},
    http::HeaderMap,
    response::IntoResponse,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::{auth::{get_current_user_from_headers, get_csrf_token_from_headers}, db, models::LeaderboardEntry, AppState};

#[derive(Template)]
#[template(path = "leaderboard.html")]
pub struct LeaderboardTemplate {
    pub entries: Vec<LeaderboardEntry>,
    pub user: Option<crate::models::User>,
    pub filter: String,
    pub csrf_token: Option<String>,
}

#[derive(Deserialize)]
pub struct LeaderboardFilter {
    pub filter: Option<String>,
}

pub async fn leaderboard(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<LeaderboardFilter>,
) -> impl IntoResponse {
    let user = get_current_user_from_headers(&headers, &state.session_manager);
    let filter = query.filter.unwrap_or_else(|| "all".to_string());

    let nominees = match db::get_nominees(&state.db, "approved").await {
        Ok(n) => n,
        Err(e) => {
            tracing::error!("Failed to fetch nominees: {}", e);
            vec![]
        }
    };

    let mut entries = Vec::new();
    for nominee in nominees {
        let (community, claw) = match db::get_vote_counts(&state.db, nominee.id).await {
            Ok(counts) => counts,
            Err(e) => {
                tracing::error!("Failed to fetch votes: {}", e);
                (0, 0)
            }
        };

        let (user_voted_comm, user_voted_claw) = if let Some(ref u) = user {
            match db::get_user_voted(&state.db, u.id, nominee.id).await {
                Ok(voted) => voted,
                Err(e) => {
                    tracing::error!("Failed to check user vote: {}", e);
                    (false, false)
                }
            }
        } else {
            (false, false)
        };

        entries.push(LeaderboardEntry {
            nominee,
            community_votes: community,
            claw_bot_votes: claw,
            user_voted_community: user_voted_comm,
            user_voted_claw,
        });
    }

    // Sort: total votes desc, then vibe_score desc
    entries.sort_by(|a, b| {
        let a_total = a.community_votes + a.claw_bot_votes;
        let b_total = b.community_votes + b.claw_bot_votes;
        let vote_cmp = b_total.cmp(&a_total);
        if vote_cmp != std::cmp::Ordering::Equal {
            return vote_cmp;
        }
        // NaN-safe descending comparison
        match (a.nominee.vibe_score.is_nan(), b.nominee.vibe_score.is_nan()) {
            (true, true) => std::cmp::Ordering::Equal,
            (true, false) => std::cmp::Ordering::Greater,
            (false, true) => std::cmp::Ordering::Less,
            (false, false) => b
                .nominee
                .vibe_score
                .partial_cmp(&a.nominee.vibe_score)
                .unwrap_or(std::cmp::Ordering::Equal),
        }
    });

    // Filter-specific stable sort (preserves total-vote/vibe ordering for ties)
    match filter.as_str() {
        "community" => entries.sort_by(|a, b| b.community_votes.cmp(&a.community_votes)),
        "claw_bot" => entries.sort_by(|a, b| b.claw_bot_votes.cmp(&a.claw_bot_votes)),
        _ => {}
    }

    crate::HtmlTemplate(LeaderboardTemplate {
        entries,
        user,
        filter,
        csrf_token: get_csrf_token_from_headers(&headers),
    })
}
