use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: i64,
    pub github_id: i64,
    pub github_login: String,
    pub avatar_url: Option<String>,
    pub is_claw_bot: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Nominee {
    pub id: i64,
    pub github_login: String,
    pub avatar_url: Option<String>,
    pub total_commits_90d: i64,
    pub total_loc_90d: i64,
    pub repo_count_90d: i64,
    pub desloppify_score: Option<f64>,
    pub vibe_score: f64,
    pub nomination_status: String,
    pub pipeline_report: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct LeaderboardEntry {
    pub nominee: Nominee,
    pub community_votes: i64,
    pub claw_bot_votes: i64,
    pub user_voted_community: bool,
    pub user_voted_claw: bool,
}

#[derive(Debug, Deserialize)]
pub struct NominationFile {
    pub github_login: String,
    pub evidence_url: Option<String>,
    pub repos: Option<Vec<NominationRepo>>,
    pub nominator: Option<Nominator>,
}

#[derive(Debug, Deserialize)]
pub struct NominationRepo {
    pub name: String,
    pub url: String,
    pub why: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Nominator {
    pub github_login: String,
    pub reason: Option<String>,
}
