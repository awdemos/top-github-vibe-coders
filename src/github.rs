use reqwest::Client;
use serde::Deserialize;

#[derive(Clone)]
pub struct GitHubClient {
    pub(crate) client: Client,
    pub(crate) token: String,
}

#[derive(Debug, Deserialize)]
pub struct GitHubUser {
    pub id: i64,
    pub login: String,
    pub avatar_url: String,
    pub bio: Option<String>,
    pub public_repos: i32,
}

#[derive(Debug, Deserialize)]
pub struct GitHubRepo {
    pub name: String,
    pub html_url: String,
    pub language: Option<String>,
    pub created_at: String,
    pub pushed_at: Option<String>,
}

impl GitHubClient {
    pub fn new(token: String) -> Self {
        Self {
            client: Client::new(),
            token,
        }
    }

    pub async fn get_user(&self, login: &str) -> anyhow::Result<GitHubUser> {
        let resp = self
            .client
            .get(format!("https://api.github.com/users/{login}"))
            .header("Authorization", format!("Bearer {}", self.token))
            .header("User-Agent", "top-github-vibe-coders")
            .header("Accept", "application/vnd.github.v3+json")
            .send()
            .await?
            .error_for_status()
            .map_err(|e| anyhow::anyhow!("GitHub API error fetching user {login}: {e}"))?
            .json::<GitHubUser>()
            .await?;
        Ok(resp)
    }

    pub async fn get_authenticated_user(&self, access_token: &str) -> anyhow::Result<GitHubUser> {
        let resp = self
            .client
            .get("https://api.github.com/user")
            .header("Authorization", format!("Bearer {access_token}"))
            .header("User-Agent", "top-github-vibe-coders")
            .header("Accept", "application/vnd.github.v3+json")
            .send()
            .await?
            .error_for_status()
            .map_err(|e| anyhow::anyhow!("GitHub API error fetching authenticated user: {e}"))?
            .json::<GitHubUser>()
            .await?;
        Ok(resp)
    }

    pub async fn get_user_repos(&self, login: &str) -> anyhow::Result<Vec<GitHubRepo>> {
        let resp = self
            .client
            .get(format!(
                "https://api.github.com/users/{login}/repos?sort=pushed&per_page=100"
            ))
            .header("Authorization", format!("Bearer {}", self.token))
            .header("User-Agent", "top-github-vibe-coders")
            .header("Accept", "application/vnd.github.v3+json")
            .send()
            .await?
            .error_for_status()
            .map_err(|e| anyhow::anyhow!("GitHub API error fetching repos for {login}: {e}"))?
            .json::<Vec<GitHubRepo>>()
            .await?;
        Ok(resp)
    }

    pub async fn get_user_events(
        &self,
        login: &str,
    ) -> anyhow::Result<Vec<serde_json::Value>> {
        let resp = self
            .client
            .get(format!(
                "https://api.github.com/users/{login}/events/public?per_page=100"
            ))
            .header("Authorization", format!("Bearer {}", self.token))
            .header("User-Agent", "top-github-vibe-coders")
            .header("Accept", "application/vnd.github.v3+json")
            .send()
            .await?
            .error_for_status()
            .map_err(|e| anyhow::anyhow!("GitHub API error fetching events for {login}: {e}"))?
            .json::<Vec<serde_json::Value>>()
            .await?;
        Ok(resp)
    }

    pub async fn is_likely_claw_bot(&self, login: &str) -> anyhow::Result<bool> {
        let user = self.get_user(login).await?;
        let repos = self.get_user_repos(login).await?;

        let bio_bot = user
            .bio
            .as_ref()
            .map(|b| {
                let b = b.to_lowercase();
                b.contains("bot")
                    || b.contains("agent")
                    || b.contains("claw")
                    || b.contains("autonomous")
                    || b.contains("ai")
            })
            .unwrap_or(false);

        let repo_bot = repos.iter().any(|r| {
            let name = r.name.to_lowercase();
            name.contains("claw")
                || name.contains("bot")
                || name.contains("agent")
                || name.contains("autonomous")
        });

        Ok(bio_bot || repo_bot)
    }
}
