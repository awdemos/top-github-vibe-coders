use reqwest::Client;
use serde::Deserialize;
use moka::future::Cache;

#[derive(Clone)]
pub struct GitHubClient {
    client: Client,
    token: String,
    base_url: String,
    user_cache: Cache<String, GitHubUser>,
    repo_cache: Cache<String, Vec<GitHubRepo>>,
    event_cache: Cache<String, Vec<serde_json::Value>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubUser {
    pub id: i64,
    pub login: String,
    pub avatar_url: String,
    pub bio: Option<String>,
    pub public_repos: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubRepo {
    pub name: String,
    pub html_url: String,
    pub language: Option<String>,
    pub created_at: String,
    pub pushed_at: Option<String>,
}

impl GitHubClient {
    pub fn new(token: String) -> Self {
        Self::with_base_url(token, "https://api.github.com".to_string())
    }

    pub fn with_base_url(token: String, base_url: String) -> Self {
        Self {
            client: Client::new(),
            token,
            base_url,
            user_cache: Cache::builder()
                .time_to_live(std::time::Duration::from_secs(300))
                .build(),
            repo_cache: Cache::builder()
                .time_to_live(std::time::Duration::from_secs(300))
                .build(),
            event_cache: Cache::builder()
                .time_to_live(std::time::Duration::from_secs(120))
                .build(),
        }
    }

    pub async fn exchange_code(
        &self,
        client_id: &str,
        client_secret: &str,
        code: &str,
    ) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .client
            .post("https://github.com/login/oauth/access_token")
            .header("Accept", "application/json")
            .form(&[
                ("client_id", client_id),
                ("client_secret", client_secret),
                ("code", code),
            ])
            .send()
            .await?
            .error_for_status()
            .map_err(|e| anyhow::anyhow!("GitHub OAuth token exchange failed: {e}"))?
            .json::<serde_json::Value>()
            .await?;
        Ok(resp)
    }

    pub async fn get_user(&self, login: &str) -> anyhow::Result<GitHubUser> {
        let login = login.to_lowercase();
        if let Some(cached) = self.user_cache.get(&login).await {
            return Ok(cached);
        }

        let resp = self
            .client
            .get(format!("{}/users/{login}", self.base_url))
            .header("Authorization", format!("Bearer {}", self.token))
            .header("User-Agent", "top-github-vibe-coders")
            .header("Accept", "application/vnd.github.v3+json")
            .send()
            .await?
            .error_for_status()
            .map_err(|e| anyhow::anyhow!("GitHub API error fetching user {login}: {e}"))?
            .json::<GitHubUser>()
            .await?;

        self.user_cache.insert(login, resp.clone()).await;
        Ok(resp)
    }

    pub async fn get_authenticated_user(&self, access_token: &str) -> anyhow::Result<GitHubUser> {
        let resp = self
            .client
            .get(format!("{}/user", self.base_url))
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
        let login = login.to_lowercase();
        if let Some(cached) = self.repo_cache.get(&login).await {
            return Ok(cached);
        }

        let resp = self
            .client
            .get(format!(
                "{}/users/{login}/repos?sort=pushed&per_page=100",
                self.base_url
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

        self.repo_cache.insert(login, resp.clone()).await;
        Ok(resp)
    }

    pub async fn get_user_events(
        &self,
        login: &str,
    ) -> anyhow::Result<Vec<serde_json::Value>> {
        let login = login.to_lowercase();
        if let Some(cached) = self.event_cache.get(&login).await {
            return Ok(cached);
        }

        let resp = self
            .client
            .get(format!(
                "{}/users/{login}/events/public?per_page=100",
                self.base_url
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

        self.event_cache.insert(login, resp.clone()).await;
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

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{MockServer, Mock, ResponseTemplate};
    use wiremock::matchers::{method, path};

    #[tokio::test]
    async fn test_get_user_caches_result() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/users/octocat"))
            .respond_with(ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({
                    "id": 1,
                    "login": "octocat",
                    "avatar_url": "https://example.com/avatar.png",
                    "bio": "A test user",
                    "public_repos": 42
                })))
            .expect(1)
            .mount(&server)
            .await;

        let client = GitHubClient::with_base_url("test-token".to_string(), server.uri());
        let user1 = client.get_user("octocat").await.unwrap();
        let user2 = client.get_user("octocat").await.unwrap();

        assert_eq!(user1.login, "octocat");
        assert_eq!(user1.id, 1);
        assert_eq!(user1.login, user2.login);
        // WireMock will verify the mock was only hit once
    }

    #[tokio::test]
    async fn test_get_user_handles_404() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/users/nonexistent"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let client = GitHubClient::with_base_url("test-token".to_string(), server.uri());
        let result = client.get_user("nonexistent").await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("404"));
    }

    #[tokio::test]
    async fn test_is_likely_claw_bot() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/users/clawbot"))
            .respond_with(ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({
                    "id": 2,
                    "login": "clawbot",
                    "avatar_url": "https://example.com/avatar.png",
                    "bio": "I am a claw bot",
                    "public_repos": 5
                })))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/users/clawbot/repos"))
            .respond_with(ResponseTemplate::new(200)
                .set_body_json(serde_json::json!([
                    {"name": "hello-world", "html_url": "https://github.com/clawbot/hello-world", "language": "Rust", "created_at": "2024-01-01T00:00:00Z"}
                ])))
            .mount(&server)
            .await;

        let client = GitHubClient::with_base_url("test-token".to_string(), server.uri());
        assert!(client.is_likely_claw_bot("clawbot").await.unwrap());
    }
}
