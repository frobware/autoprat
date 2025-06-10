use std::process::Command;

use anyhow::{Context, Result};
use octocrab::Octocrab;

pub fn get_github_token() -> Result<String> {
    // Prefer environment variables over gh CLI to avoid subprocess overhead.
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        return Ok(token);
    }

    if let Ok(token) = std::env::var("GH_TOKEN") {
        return Ok(token);
    }

    let output = Command::new("gh").args(["auth", "token"]).output()?;

    if !output.status.success() {
        anyhow::bail!("Failed to get GitHub token from gh CLI. Please run 'gh auth login' first");
    }

    let token = String::from_utf8(output.stdout)?.trim().to_string();

    if token.is_empty() {
        anyhow::bail!("Empty token returned from gh CLI");
    }

    Ok(token)
}

/// Creates an authenticated GitHub client using available credentials.
pub async fn setup_github_client() -> Result<Octocrab> {
    let token = get_github_token().context("Failed to obtain GitHub authentication token")?;
    Octocrab::builder()
        .personal_token(token)
        .build()
        .context("Failed to create GitHub client")
}

pub fn parse_repo_from_string(repo: &str) -> Result<(&str, &str)> {
    let parts: Vec<&str> = repo.split('/').collect();
    if parts.len() != 2 {
        anyhow::bail!("Repository must be in format 'owner/repo', got: '{}'", repo);
    }
    Ok((parts[0], parts[1]))
}

pub fn parse_pr_url(url_str: &str) -> Result<(String, String, u64)> {
    let url =
        url::Url::parse(url_str).with_context(|| format!("Failed to parse URL: '{}'", url_str))?;

    if url.host_str() != Some("github.com") {
        anyhow::bail!("URL must be a GitHub PR URL, got: '{}'", url_str);
    }

    let path_segments: Vec<&str> = url
        .path_segments()
        .context("Cannot parse URL path")?
        .collect();

    // Validate path structure: ["owner", "repo", "pull", "123"]
    if path_segments.len() != 4 || path_segments[2] != "pull" {
        anyhow::bail!(
            "URL must be in format https://github.com/owner/repo/pull/123, got: '{}'",
            url_str
        );
    }

    let owner = path_segments[0].to_string();
    let repo = path_segments[1].to_string();
    let pr_number: u64 = path_segments[3]
        .parse()
        .with_context(|| format!("Invalid PR number in URL: '{}'", url_str))?;

    Ok((owner, repo, pr_number))
}

/// Extracts repository owner and name from a GitHub PR URL.
pub fn extract_repo_info_from_url(url: &str) -> Result<(String, String)> {
    let url_parts: Vec<&str> = url.split('/').collect();
    if url_parts.len() < 5 {
        anyhow::bail!("Invalid PR URL format: '{}'", url);
    }
    Ok((url_parts[3].to_string(), url_parts[4].to_string()))
}
