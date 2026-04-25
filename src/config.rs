use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub turso_auth_token: String,
    pub github_client_id: String,
    pub github_client_secret: String,
    pub github_token: String,
    pub session_secret: String,
    pub app_url: String,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            database_url: env::var("DATABASE_URL")?,
            turso_auth_token: env::var("TURSO_AUTH_TOKEN")?,
            github_client_id: env::var("GITHUB_CLIENT_ID")?,
            github_client_secret: env::var("GITHUB_CLIENT_SECRET")?,
            github_token: env::var("GITHUB_TOKEN")?,
            session_secret: env::var("SESSION_SECRET")?,
            app_url: env::var("APP_URL").unwrap_or_else(|_| "http://localhost:3000".to_string()),
        })
    }
}
