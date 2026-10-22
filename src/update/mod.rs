//! Self-update: `runsite upgrade` and the once-a-day "new version" notice.

pub mod artifact;
pub mod install;
pub mod notice;

use anyhow::{Context, Result};
use futures_util::StreamExt;
use serde::Deserialize;
use std::time::Duration;

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const GITHUB_API: &str = "https://api.github.com";
const DEFAULT_REPO: &str = "runsite-platform/runsite-cli";

/// Where releases are published.
#[derive(Clone, Debug)]
pub struct ReleaseSource {
    api_base: String,
    repo: String,
}

impl ReleaseSource {
    /// GitHub releases of this CLI; `RUNSITE_REPO` overrides the repository,
    /// as it does for the installers.
    pub fn github() -> Self {
        let repo = std::env::var("RUNSITE_REPO")
            .ok()
            .filter(|repo| !repo.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_REPO.to_string());
        Self {
            api_base: GITHUB_API.to_string(),
            repo,
        }
    }

    /// A GitHub-compatible API at `api_base`, e.g. a mock server.
    #[cfg(test)]
    pub fn at(api_base: &str, repo: &str) -> Self {
        Self {
            api_base: api_base.to_string(),
            repo: repo.to_string(),
        }
    }

    /// The newest published release. GitHub's `latest` skips drafts and pre-releases.
    pub async fn latest(&self, client: &reqwest::Client) -> Result<Release> {
        fetch_latest_release(client, &self.api_base, &self.repo).await
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Release {
    pub tag_name: String,
    #[serde(default)]
    pub assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseAsset {
    pub name: String,
    pub browser_download_url: String,
}

impl Release {
    pub fn asset(&self, name: &str) -> Option<&ReleaseAsset> {
        self.assets.iter().find(|asset| asset.name == name)
    }
}

pub fn http_client(timeout: Duration) -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(crate::USER_AGENT)
        .timeout(timeout)
        .build()
        .context("failed to build HTTP client")
}

async fn fetch_latest_release(
    client: &reqwest::Client,
    api_base: &str,
    repo: &str,
) -> Result<Release> {
    let url = format!(
        "{}/repos/{}/releases/latest",
        api_base.trim_end_matches('/'),
        repo
    );
    let response = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .context("could not reach GitHub to check for updates")?;
    let status = response.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        anyhow::bail!("no published releases found for {repo}");
    }
    if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::TOO_MANY_REQUESTS
    {
        anyhow::bail!("GitHub rate limit reached, try again in an hour");
    }
    if !status.is_success() {
        anyhow::bail!("GitHub returned {status} while checking for updates");
    }
    response
        .json()
        .await
        .context("unexpected response from GitHub releases API")
}

/// `major.minor.patch` from a tag like `v0.4.0`; pre-release suffixes are ignored.
pub fn parse_version(tag: &str) -> Option<(u64, u64, u64)> {
    let core = version_of(tag).split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(candidate), Some(current)) => candidate > current,
        _ => false,
    }
}

/// The version a tag names: `v0.4.0` is `0.4.0`.
pub fn version_of(tag: &str) -> &str {
    tag.trim().trim_start_matches('v')
}

/// Download a file into memory, refusing anything larger than `max_bytes`.
pub async fn download(client: &reqwest::Client, url: &str, max_bytes: usize) -> Result<Vec<u8>> {
    let response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("download failed: {url}"))?
        .error_for_status()
        .with_context(|| format!("download failed: {url}"))?;
    if response
        .content_length()
        .is_some_and(|length| length > max_bytes as u64)
    {
        anyhow::bail!("download is larger than expected: {url}");
    }
    let mut body = Vec::new();
    let mut chunks = response.bytes_stream();
    while let Some(chunk) = chunks.next().await {
        let chunk = chunk.with_context(|| format!("download failed: {url}"))?;
        if body.len() + chunk.len() > max_bytes {
            anyhow::bail!("download is larger than expected: {url}");
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn versions_compare_numerically() {
        assert!(is_newer("v0.10.0", "0.9.9"));
        assert!(is_newer("v1.0.0", "0.3.0"));
        assert!(!is_newer("v0.3.0", "0.3.0"));
        assert!(!is_newer("v0.2.9", "0.3.0"));
        assert!(!is_newer("nightly", "0.3.0"));
        assert_eq!(parse_version("v0.4.0-rc.1"), Some((0, 4, 0)));
        assert_eq!(parse_version("0.4"), None);
        assert_eq!(parse_version("1.2.3.4"), None);
    }

    #[tokio::test]
    async fn fetches_the_latest_release_with_a_user_agent() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/acme/cli/releases/latest"))
            .and(header(
                "user-agent",
                format!("runsite-cli/{CURRENT_VERSION}").as_str(),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "tag_name": "v9.9.9",
                "assets": [{"name": "SHA256SUMS.txt", "browser_download_url": "https://x/sums"}]
            })))
            .mount(&server)
            .await;

        let client = http_client(Duration::from_secs(5)).unwrap();
        let release = ReleaseSource::at(&server.uri(), "acme/cli")
            .latest(&client)
            .await
            .unwrap();
        assert_eq!(release.tag_name, "v9.9.9");
        assert!(release.asset("SHA256SUMS.txt").is_some());
    }

    #[tokio::test]
    async fn explains_a_rate_limit() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;

        let client = http_client(Duration::from_secs(5)).unwrap();
        let error = fetch_latest_release(&client, &server.uri(), "acme/cli")
            .await
            .unwrap_err();
        assert!(error.to_string().contains("rate limit"));
    }

    #[tokio::test]
    async fn refuses_oversized_downloads() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0u8; 64]))
            .mount(&server)
            .await;

        let client = http_client(Duration::from_secs(5)).unwrap();
        let url = format!("{}/big", server.uri());
        assert!(download(&client, &url, 32).await.is_err());
        assert_eq!(download(&client, &url, 64).await.unwrap().len(), 64);
    }
}
