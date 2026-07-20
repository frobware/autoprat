use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use url::Url;

use crate::{pr_selector::PrIdentifier, search::FetchPlan};

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

#[derive(Debug, Clone, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepoUrlError {
    InvalidUrl {
        input: String,
        source: url::ParseError,
    },
    CannotParsePath {
        input: String,
    },
    MissingOwnerAndRepository {
        input: String,
    },
    InvalidRepository {
        input: String,
        source: RepoError,
    },
}

impl std::fmt::Display for RepoUrlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RepoUrlError::InvalidUrl { input, source } => {
                write!(f, "Failed to parse URL: '{input}': {source}")
            }
            RepoUrlError::CannotParsePath { input } => {
                write!(f, "Cannot parse URL path: '{input}'")
            }
            RepoUrlError::MissingOwnerAndRepository { input } => {
                write!(f, "URL must contain owner and repository name: '{input}'")
            }
            RepoUrlError::InvalidRepository { input, source } => {
                write!(f, "Invalid repository in URL '{input}': {source}")
            }
        }
    }
}

impl std::error::Error for RepoUrlError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            RepoUrlError::InvalidUrl { source, .. } => Some(source),
            RepoUrlError::InvalidRepository { source, .. } => Some(source),
            RepoUrlError::CannotParsePath { .. }
            | RepoUrlError::MissingOwnerAndRepository { .. } => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedForgeUrl {
    pub(crate) repo: Repo,
    pub(crate) path_segments: Vec<String>,
}

/// Repository identifier with owner and name.
///
/// Represents a repository in "owner/name" format. Validates that
/// neither component is empty or contains forward slashes.
#[derive(Debug, Clone, PartialEq, Eq)]
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

    /// Parse repository information from any Git hosting URL.
    ///
    /// This parser intentionally returns only repository identity. Pull request
    /// selectors parse the PR number in `pr_selector`, where a missing PR
    /// number is an error rather than an optional field.
    /// Works with URLs like:
    /// - https://github.com/owner/repo → repo
    /// - https://github.com/owner/repo/pull/123 → repo
    /// - https://gitlab.com/owner/repo/merge_requests/456 → repo
    /// - https://api.github.com/repos/owner/repo/pulls/123 → repo
    pub fn parse_url(url_str: &str) -> std::result::Result<Self, RepoUrlError> {
        parse_forge_url(url_str).map(|parsed| parsed.repo)
    }

    pub fn owner(&self) -> &str {
        &self.owner
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

pub(crate) fn parse_forge_url(url_str: &str) -> std::result::Result<ParsedForgeUrl, RepoUrlError> {
    let url = Url::parse(url_str).map_err(|source| RepoUrlError::InvalidUrl {
        input: url_str.to_string(),
        source,
    })?;

    let path_segments: Vec<String> = url
        .path_segments()
        .ok_or_else(|| RepoUrlError::CannotParsePath {
            input: url_str.to_string(),
        })?
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();

    let (owner, repo_name) = if path_segments
        .first()
        .is_some_and(|segment| segment == "repos")
        && path_segments.len() >= 3
    {
        // Handle API URLs like https://api.github.com/repos/owner/repo/...
        (&path_segments[1], &path_segments[2])
    } else if path_segments.len() >= 2 {
        // Handle regular URLs like https://github.com/owner/repo/...
        (&path_segments[0], &path_segments[1])
    } else {
        return Err(RepoUrlError::MissingOwnerAndRepository {
            input: url_str.to_string(),
        });
    };

    let repo = Repo::new(owner, repo_name).map_err(|source| RepoUrlError::InvalidRepository {
        input: url_str.to_string(),
        source,
    })?;

    Ok(ParsedForgeUrl {
        repo,
        path_segments,
    })
}

impl std::fmt::Display for Repo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.owner, self.name)
    }
}

/// Output format for displaying pull request information.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// Lifecycle state of a pull request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrState {
    Open,
    Closed,
    Merged,
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
    pub author_simple_name: String,
    pub url: String,
    pub labels: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub base_branch: String,
    pub commit_count: u64,
    pub is_draft: bool,
    pub state: PrState,

    // Associated data.
    pub checks: Vec<CheckInfo>,
    pub recent_comments: Vec<CommentInfo>,
}

impl PullRequest {
    pub fn matches_author(&self, author: &str) -> bool {
        self.author_login == author
            || self.author_simple_name == author
            || (self.author_login.starts_with(&format!("{author}["))
                && self.author_login.ends_with("]"))
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

    pub fn matches_base_branch(&self, branch: &str) -> bool {
        self.base_branch == branch
    }
}

/// Forge-neutral criterion used both for server-side narrowing and local checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchCriterion {
    MissingLabel(String),
    PresentLabel(String),
    BaseBranch(String),
}

impl SearchCriterion {
    pub fn matches(&self, pr: &PullRequest) -> bool {
        match self {
            Self::MissingLabel(label) => !pr.has_label(label),
            Self::PresentLabel(label) => pr.has_label(label),
            Self::BaseBranch(branch) => pr.matches_base_branch(branch),
        }
    }
}

/// Filter applied after fetching pull requests.
///
/// Performs client-side filtering on the complete PR data, allowing
/// checks that cannot be expressed in GitHub's search syntax.
pub trait PostFilter: std::fmt::Debug + Send + Sync {
    fn matches(&self, pr: &PullRequest) -> bool;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommentAction {
    Approve,
    Lgtm,
    OkToTest,
    Retest,
    Hold,
    Custom(String),
}

impl CommentAction {
    pub fn name(&self) -> &'static str {
        match self {
            CommentAction::Approve => "approve",
            CommentAction::Lgtm => "lgtm",
            CommentAction::OkToTest => "ok-to-test",
            CommentAction::Retest => "retest",
            CommentAction::Hold => "hold",
            CommentAction::Custom(_) => "custom-comment",
        }
    }

    pub fn body(&self) -> &str {
        match self {
            CommentAction::Approve => "/approve",
            CommentAction::Lgtm => "/lgtm",
            CommentAction::OkToTest => "/ok-to-test",
            CommentAction::Retest => "/retest",
            CommentAction::Hold => "/hold",
            CommentAction::Custom(comment) => comment,
        }
    }

    pub fn only_if(&self, pr: &PullRequest) -> bool {
        match self {
            CommentAction::Approve => !pr.has_label("approved"),
            CommentAction::Lgtm => !pr.has_label("lgtm"),
            CommentAction::OkToTest => pr.has_label("needs-ok-to-test"),
            CommentAction::Retest | CommentAction::Custom(_) => true,
            CommentAction::Hold => !pr.has_label("do-not-merge/hold"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrAction {
    Comment(CommentAction),
    GroupedComment(Vec<CommentAction>),
    Close,
    Merge,
}

impl PrAction {
    pub fn comment(action: CommentAction) -> Self {
        Self::Comment(action)
    }

    pub fn comments(actions: Vec<CommentAction>) -> Option<Self> {
        match actions.len() {
            0 => None,
            1 => actions.into_iter().next().map(Self::Comment),
            _ => Some(Self::GroupedComment(actions)),
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            PrAction::Comment(action) => action.name(),
            PrAction::GroupedComment(_) => "grouped-comment",
            PrAction::Close => "close",
            PrAction::Merge => "merge",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Task {
    pub pr_info: PullRequest,
    pub action: PrAction,
}

/// Abstraction for version control forges (GitHub, GitLab, etc.).
///
/// Provides a common interface for fetching pull requests from different
/// platforms. Currently only GitHub is implemented.
#[async_trait]
pub trait Forge {
    async fn fetch_pull_requests(&self, plan: &FetchPlan) -> Result<Vec<PullRequest>>;
}

#[derive(Debug)]
pub struct FetchCriteria {
    pub repos: Vec<Repo>,
    pub prs: Vec<PrIdentifier>,
    pub query: Option<String>,
    pub limit: usize,
    pub search_criteria: Vec<SearchCriterion>,
}

#[derive(Debug)]
pub struct SelectionPolicy {
    pub exclude: Vec<PrIdentifier>,
    pub post_filters: Vec<Box<dyn PostFilter + Send + Sync>>,
}

#[derive(Debug)]
pub struct ActionPolicy {
    pub actions: Vec<PrAction>,
    pub throttle: Option<Duration>,
    pub history_max_age: Duration,
    pub history_max_comments: usize,
    pub commit_limit: u64,
}

impl ActionPolicy {
    pub fn has_actions(&self) -> bool {
        !self.actions.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplaySettings {
    pub mode: DisplayMode,
    pub truncate_titles: bool,
}

/// Specification for querying and processing pull requests.
#[derive(Debug)]
pub struct QuerySpec {
    pub fetch: FetchCriteria,
    pub selection: SelectionPolicy,
    pub action_policy: ActionPolicy,
}

#[derive(Debug)]
pub struct AppRequest {
    pub query: QuerySpec,
    pub display: DisplaySettings,
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
    use chrono::{TimeZone, Utc};

    use super::*;

    fn pr_with_authors(author_login: &str, author_simple_name: &str) -> PullRequest {
        PullRequest {
            repo: Repo::new("owner", "repo").unwrap(),
            number: 123,
            title: "Test PR".to_string(),
            author_login: author_login.to_string(),
            author_simple_name: author_simple_name.to_string(),
            url: "https://github.com/owner/repo/pull/123".to_string(),
            labels: vec![],
            created_at: Utc.with_ymd_and_hms(2026, 5, 29, 12, 0, 0).unwrap(),
            base_branch: "main".to_string(),
            commit_count: 1,
            is_draft: false,
            state: PrState::Open,
            checks: vec![],
            recent_comments: vec![],
        }
    }

    #[test]
    fn matches_author_isolates_login_simple_name_and_bot_forms() {
        // Login matches while the simple name differs: pins the first
        // `||` in the alternation.
        assert!(pr_with_authors("alice", "alice-display").matches_author("alice"));

        // Simple name matches while the login differs: pins the second
        // `||`.
        assert!(pr_with_authors("alice-login", "alice").matches_author("alice"));

        // A bot login matches the bare bot name via the bracket branch.
        assert!(pr_with_authors("dependabot[bot]", "dependabot[bot]").matches_author("dependabot"));

        // None of the three forms match.
        assert!(!pr_with_authors("alice", "alice-display").matches_author("bob"));
    }

    #[test]
    fn test_parse_url_formats() {
        let test_cases = [
            ("https://github.com/owner/repo", "owner", "repo"),
            ("https://github.com/owner/repo/pull/123", "owner", "repo"),
            (
                "https://api.github.com/repos/owner/repo/pulls/123",
                "owner",
                "repo",
            ),
            (
                "https://github.enterprise.com/owner/repo/pull/456",
                "owner",
                "repo",
            ),
            (
                "https://gitlab.com/owner/repo/merge_requests/789",
                "owner",
                "repo",
            ),
            (
                "https://bitbucket.org/owner/repo/pull-requests/321",
                "owner",
                "repo",
            ),
        ];

        for (url, expected_owner, expected_name) in test_cases {
            let repo = Repo::parse_url(url).unwrap();
            assert_eq!(repo.owner(), expected_owner, "Failed for URL: {}", url);
            assert_eq!(repo.name(), expected_name, "Failed for URL: {}", url);
        }
    }

    #[test]
    fn test_parse_url_edge_cases() {
        let test_cases = [
            ("https://github.com/owner/repo/", "owner", "repo"),
            (
                "https://github.com/owner/repo/pull/789/files",
                "owner",
                "repo",
            ),
            ("https://github.com/owner/repo/issues/123", "owner", "repo"),
        ];

        for (url, expected_owner, expected_name) in test_cases {
            let repo = Repo::parse_url(url).unwrap();
            assert_eq!(repo.owner(), expected_owner, "Failed for URL: {}", url);
            assert_eq!(repo.name(), expected_name, "Failed for URL: {}", url);
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
