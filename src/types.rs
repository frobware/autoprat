use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use url::Url;

/// Error types for validation
#[derive(Debug, Clone, PartialEq)]
pub enum CheckNameError {
    Empty,
}

impl std::fmt::Display for CheckNameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CheckNameError::Empty => write!(f, "Check name cannot be empty"),
        }
    }
}

impl std::error::Error for CheckNameError {}

#[derive(Debug, Clone, PartialEq)]
pub enum LogUrlError {
    InvalidUrl(String),
}

impl std::fmt::Display for LogUrlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogUrlError::InvalidUrl(msg) => write!(f, "Invalid log URL: {msg}"),
        }
    }
}

impl std::error::Error for LogUrlError {}

/// A validated check name that cannot be empty.
///
/// Enforces non-empty check names through the type system using
/// parse-don't-validate pattern. Whitespace is trimmed on creation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CheckName(String);

impl CheckName {
    pub fn new(name: impl AsRef<str>) -> Result<Self, CheckNameError> {
        let name = name.as_ref().trim();
        if name.is_empty() {
            return Err(CheckNameError::Empty);
        }
        Ok(Self(name.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for CheckName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A typed wrapper for check URLs using parse-don't-validate
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CheckUrl(Url);

impl CheckUrl {
    pub fn new(url: impl AsRef<str>) -> Result<Self, url::ParseError> {
        Ok(Self(Url::parse(url.as_ref())?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    pub fn host(&self) -> Option<&str> {
        self.0.host_str()
    }

    pub fn path(&self) -> &str {
        self.0.path()
    }

    pub fn scheme(&self) -> &str {
        self.0.scheme()
    }

    /// Get the underlying URL
    pub fn as_url(&self) -> &Url {
        &self.0
    }
}

impl std::fmt::Display for CheckUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A validated URL specifically for build logs.
///
/// Ensures URLs are valid and use HTTP/HTTPS schemes. Provides typed
/// access to URL components for log analysis.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LogUrl(Url);

impl LogUrl {
    pub fn new(url: impl AsRef<str>) -> Result<Self, LogUrlError> {
        let parsed =
            Url::parse(url.as_ref()).map_err(|e| LogUrlError::InvalidUrl(e.to_string()))?;

        // Additional validation for log URLs.
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(LogUrlError::InvalidUrl(
                "Log URLs must use HTTP or HTTPS".to_string(),
            ));
        }

        Ok(Self(parsed))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    pub fn host(&self) -> Option<&str> {
        self.0.host_str()
    }

    pub fn path(&self) -> &str {
        self.0.path()
    }

    /// Get the underlying URL
    pub fn as_url(&self) -> &Url {
        &self.0
    }
}

impl std::fmt::Display for LogUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RepoError {
    EmptyField,
    InvalidCharacter,
    InvalidFormat,
}

impl std::fmt::Display for RepoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RepoError::EmptyField => write!(f, "Repository owner and name cannot be empty"),
            RepoError::InvalidCharacter => {
                write!(f, "Repository owner and name cannot contain '/'")
            }
            RepoError::InvalidFormat => write!(f, "Repository must be in 'owner/name' format"),
        }
    }
}

impl std::error::Error for RepoError {}

/// Repository identifier with owner and name.
///
/// Represents a repository in "owner/name" format. Validates that
/// neither component is empty or contains forward slashes.
#[derive(Debug, Clone, PartialEq)]
pub struct Repo {
    owner: String,
    name: String,
}

impl Repo {
    pub fn new(owner: impl AsRef<str>, name: impl AsRef<str>) -> Result<Self, RepoError> {
        let owner = owner.as_ref().trim();
        let name = name.as_ref().trim();

        if owner.is_empty() || name.is_empty() {
            return Err(RepoError::EmptyField);
        }

        if owner.contains('/') || name.contains('/') {
            return Err(RepoError::InvalidCharacter);
        }

        Ok(Self {
            owner: owner.to_string(),
            name: name.to_string(),
        })
    }

    pub fn parse(s: &str) -> Result<Self, RepoError> {
        let parts: Vec<&str> = s.split('/').collect();
        if parts.len() != 2 {
            return Err(RepoError::InvalidFormat);
        }
        Self::new(parts[0], parts[1])
    }

    /// Parse repository information from any Git hosting URL
    /// Returns (Repo, Option<u64>) where the u64 is the PR/MR number if present
    /// Works with URLs like:
    /// - https://github.com/owner/repo → (repo, None)
    /// - https://github.com/owner/repo/pull/123 → (repo, Some(123))
    /// - https://gitlab.com/owner/repo/merge_requests/456 → (repo, Some(456))
    /// - https://api.github.com/repos/owner/repo/pulls/123 → (repo, Some(123))
    pub fn parse_url(url_str: &str) -> Result<(Self, Option<u64>)> {
        use anyhow::Context;

        let url =
            Url::parse(url_str).with_context(|| format!("Failed to parse URL: '{url_str}'"))?;

        let path_segments: Vec<&str> = url
            .path_segments()
            .context("Cannot parse URL path")?
            .filter(|s| !s.is_empty())
            .collect();

        let (owner, repo_name) =
            if path_segments.first() == Some(&"repos") && path_segments.len() >= 3 {
                // Handle API URLs like https://api.github.com/repos/owner/repo/...
                (path_segments[1], path_segments[2])
            } else if path_segments.len() >= 2 {
                // Handle regular URLs like https://github.com/owner/repo/...
                (path_segments[0], path_segments[1])
            } else {
                anyhow::bail!("URL must contain owner and repository name: '{}'", url_str);
            };

        let repo = Self::new(owner, repo_name)
            .map_err(|e| anyhow::anyhow!("Invalid repository in URL '{}': {}", url_str, e))?;

        // Look for PR/MR number in path - be liberal about patterns.
        let pr_number = Self::extract_pr_number(&path_segments);

        Ok((repo, pr_number))
    }

    /// Extract PR/MR number from path segments if present
    /// Supports various Git hosting patterns: pull, pulls, merge_requests, pull-requests
    fn extract_pr_number(path_segments: &[&str]) -> Option<u64> {
        let pr_keywords = ["pull", "pulls", "merge_requests", "pull-requests"];

        for keyword in &pr_keywords {
            if let Some(index) = path_segments
                .iter()
                .position(|&segment| segment == *keyword)
                && index + 1 < path_segments.len()
                && let Ok(number) = path_segments[index + 1].parse::<u64>()
            {
                return Some(number);
            }
        }
        None
    }

    pub fn owner(&self) -> &str {
        &self.owner
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn build_search_query(
        &self,
        search_filters: &[Box<dyn SearchFilter + Send + Sync>],
    ) -> String {
        let mut parts = Vec::with_capacity(search_filters.len() + 3); // Base terms plus filters.

        parts.push(format!("repo:{self}"));

        for sf in search_filters {
            sf.apply(&mut parts);
        }

        parts.push("type:pr".to_string());
        parts.push("state:open".to_string());

        parts.join(" ")
    }
}

impl std::fmt::Display for Repo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.owner, self.name)
    }
}

/// Output format for displaying pull request information.
#[derive(Debug, Clone, PartialEq)]
pub enum DisplayMode {
    Normal,
    Quiet,
    Detailed,
    DetailedWithLogs,
}

/// Final outcome of a completed CI check.
#[derive(Debug, Clone, PartialEq)]
pub enum CheckConclusion {
    Success,
    Failure,
    Cancelled,
    TimedOut,
    ActionRequired,
    Neutral,
    Skipped,
}

/// Current state of a CI status check.
#[derive(Debug, Clone, PartialEq)]
pub enum CheckState {
    Success,
    Failure,
    Pending,
    Error,
}

/// Status of a GitHub Check Run (before it completes).
#[derive(Debug, Clone, PartialEq)]
pub enum CheckRunStatus {
    Queued,
    InProgress,
    Completed,
    Waiting,
    Requested,
    Pending,
}

/// Information about a CI check or status.
///
/// Represents either a GitHub Check Run (with conclusion and run_status) or a
/// Status Context (with state). May include a URL to detailed logs.
#[derive(Debug, Clone)]
pub struct CheckInfo {
    pub name: CheckName,
    pub conclusion: Option<CheckConclusion>,
    pub run_status: Option<CheckRunStatus>,
    pub status_state: Option<CheckState>,
    pub url: Option<CheckUrl>,
}

impl CheckInfo {
    pub fn is_failed(&self) -> bool {
        matches!(
            self.conclusion,
            Some(CheckConclusion::Failure | CheckConclusion::Cancelled | CheckConclusion::TimedOut)
        ) || matches!(
            self.status_state,
            Some(CheckState::Failure | CheckState::Error)
        )
    }
}

/// Comment on a pull request.
#[derive(Debug, Clone)]
pub struct CommentInfo {
    pub body: String,
    pub created_at: DateTime<Utc>,
}

/// Complete information about a pull request.
///
/// Contains core PR metadata, CI check results, labels, and recent
/// comments. Provides methods for filtering and matching criteria.
#[derive(Debug, Clone)]
pub struct PullRequest {
    // Repository context.
    pub repo: Repo,

    // Core PR data.
    pub number: u64,
    pub title: String,
    pub author_login: String,
    pub author_search_format: String,
    pub author_simple_name: String,
    pub url: String,
    pub labels: Vec<String>,
    pub created_at: DateTime<Utc>,

    // Associated data.
    pub checks: Vec<CheckInfo>,
    pub recent_comments: Vec<CommentInfo>,
}

impl PullRequest {
    fn matches_repo_and_number(&self, repo: &Repo, number: u64) -> bool {
        self.repo == *repo && self.number == number
    }

    pub fn matches_author(&self, author: &str) -> bool {
        self.author_login == author
            || self.author_search_format == author
            || (self.author_login.starts_with(&format!("{author}["))
                && self.author_login.ends_with("]"))
            || (self.author_search_format == format!("app/{author}"))
    }

    pub fn has_failing_ci(&self) -> bool {
        self.checks.iter().any(|check| check.is_failed())
    }

    pub fn has_failing_check(&self, name: &str) -> bool {
        self.checks
            .iter()
            .any(|c| c.name.as_str() == name && c.is_failed())
    }

    pub fn has_label(&self, label: &str) -> bool {
        self.labels.iter().any(|l| l == label)
    }

    pub fn was_comment_posted_recently(
        &self,
        comment_body: &str,
        throttle_duration: Duration,
    ) -> bool {
        use chrono::Utc;

        let now = Utc::now();
        let throttle_seconds = throttle_duration.as_secs();
        let cutoff_time = now - chrono::Duration::seconds(throttle_seconds as i64);

        let target_command = comment_body.trim();

        self.recent_comments.iter().any(|comment| {
            comment.created_at > cutoff_time
                && comment
                    .body
                    .lines()
                    .any(|line| line.trim() == target_command)
        })
    }

    pub fn matches_request(&self, request: &QuerySpec) -> bool {
        // Check if this PR is explicitly excluded
        let is_excluded = request
            .exclude
            .iter()
            .any(|(repo, number)| self.matches_repo_and_number(repo, *number));

        if is_excluded {
            return false;
        }

        (request.prs.is_empty()
            || request
                .prs
                .iter()
                .any(|(repo, number)| self.matches_repo_and_number(repo, *number)))
            && request.search_filters.iter().all(|sf| sf.matches(self))
            && request.post_filters.iter().all(|pf| pf.matches(self))
    }
}

/// Filter applied during GitHub search query construction.
///
/// Modifies the search query sent to GitHub to limit results server-side.
/// The `matches` method is primarily for testing and mocking.
pub trait SearchFilter: std::fmt::Debug + Send + Sync {
    fn apply(&self, terms: &mut Vec<String>);

    /// Default is “always true” so you only need to override it when
    /// mocking.
    fn matches(&self, _pr: &PullRequest) -> bool {
        true
    }
}

/// Filter applied after fetching pull requests.
///
/// Performs client-side filtering on the complete PR data, allowing
/// checks that cannot be expressed in GitHub's search syntax.
pub trait PostFilter: std::fmt::Debug + Send + Sync {
    fn matches(&self, pr: &PullRequest) -> bool;
}

/// Action that can be performed on a pull request.
///
/// Defines conditions for execution and optional comment text. Actions
/// are only executed if `only_if` returns true for the specific PR.
pub trait Action: std::fmt::Debug + Send + Sync {
    fn name(&self) -> &'static str;
    fn only_if(&self, pr_info: &PullRequest) -> bool;
    fn get_comment_body(&self) -> Option<&str>;
    fn clone_box(&self) -> Box<dyn Action + Send + Sync>;
}

#[derive(Debug)]
/// Represents an actionable task to be performed on a specific pull request.
///
/// A Task pairs a PullRequest with an Action, representing work that should be
/// executed such as posting comments, approving, or running specific commands.
pub struct Task {
    pub pr_info: PullRequest,
    pub action: Box<dyn Action + Send + Sync>,
}

impl Clone for Box<dyn Action + Send + Sync> {
    fn clone(&self) -> Box<dyn Action + Send + Sync> {
        self.clone_box()
    }
}

/// Abstraction for version control forges (GitHub, GitLab, etc.).
///
/// Provides a common interface for fetching pull requests from different
/// platforms. Currently only GitHub is implemented.
#[async_trait]
pub trait Forge {
    async fn fetch_pull_requests(&self, spec: &QuerySpec) -> Result<Vec<PullRequest>>;
}

/// Specification for querying and processing pull requests.
///
/// Contains search criteria, filters, actions to perform, and execution
/// parameters like throttling. Supports both specific PR lookups and
/// broad searches.
#[derive(Debug)]
pub struct QuerySpec {
    pub repos: Vec<Repo>,
    pub prs: Vec<(Repo, u64)>,
    pub exclude: Vec<(Repo, u64)>,
    pub query: Option<String>,
    pub limit: usize,
    pub search_filters: Vec<Box<dyn SearchFilter + Send + Sync>>,
    pub post_filters: Vec<Box<dyn PostFilter + Send + Sync>>,
    pub actions: Vec<Box<dyn Action + Send + Sync>>,
    pub custom_comments: Vec<String>,
    pub throttle: Option<Duration>,
    pub truncate_titles: bool,
}

impl QuerySpec {
    pub fn has_actions(&self) -> bool {
        !self.actions.is_empty() || !self.custom_comments.is_empty()
    }
}

/// Result of executing a pull request query.
///
/// Contains the filtered pull requests and a list of executable actions
/// based on the query specification and PR states.
#[derive(Debug)]
pub struct QueryResult {
    pub filtered_prs: Vec<PullRequest>,
    pub executable_actions: Vec<Task>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_url_formats() {
        let test_cases = [
            // (url, expected_owner, expected_name, expected_pr_num).
            ("https://github.com/owner/repo", "owner", "repo", None),
            (
                "https://github.com/owner/repo/pull/123",
                "owner",
                "repo",
                Some(123),
            ),
            (
                "https://api.github.com/repos/owner/repo/pulls/123",
                "owner",
                "repo",
                Some(123),
            ),
            (
                "https://github.enterprise.com/owner/repo/pull/456",
                "owner",
                "repo",
                Some(456),
            ),
            (
                "https://gitlab.com/owner/repo/merge_requests/789",
                "owner",
                "repo",
                Some(789),
            ),
            (
                "https://bitbucket.org/owner/repo/pull-requests/321",
                "owner",
                "repo",
                Some(321),
            ),
        ];

        for (url, expected_owner, expected_name, expected_pr_num) in test_cases {
            let (repo, pr_num) = Repo::parse_url(url).unwrap();
            assert_eq!(repo.owner(), expected_owner, "Failed for URL: {}", url);
            assert_eq!(repo.name(), expected_name, "Failed for URL: {}", url);
            assert_eq!(pr_num, expected_pr_num, "Failed for URL: {}", url);
        }
    }

    #[test]
    fn test_parse_url_edge_cases() {
        let test_cases = [
            // (url, expected_owner, expected_name, expected_pr_num).
            ("https://github.com/owner/repo/", "owner", "repo", None),
            (
                "https://github.com/owner/repo/pull/789/files",
                "owner",
                "repo",
                Some(789),
            ),
            (
                "https://github.com/owner/repo/issues/123",
                "owner",
                "repo",
                None,
            ), // Issues don't count as PR numbers.
        ];

        for (url, expected_owner, expected_name, expected_pr_num) in test_cases {
            let (repo, pr_num) = Repo::parse_url(url).unwrap();
            assert_eq!(repo.owner(), expected_owner, "Failed for URL: {}", url);
            assert_eq!(repo.name(), expected_name, "Failed for URL: {}", url);
            assert_eq!(pr_num, expected_pr_num, "Failed for URL: {}", url);
        }
    }

    #[test]
    fn test_parse_url_error_cases() {
        let invalid_urls = [
            "not-a-url",
            "https://github.com/owner",
            "https://github.com/",
        ];

        for url in invalid_urls {
            assert!(
                Repo::parse_url(url).is_err(),
                "Expected error for URL: {}",
                url
            );
        }
    }
}
