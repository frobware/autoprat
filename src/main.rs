use std::process::Command;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use clap::Parser;
use comfy_table::{Cell, Table, presets::NOTHING};
use octocrab::{
    Octocrab,
    models::{StatusState, workflows::Conclusion},
};
use serde::{Deserialize, Deserializer};
// Include build-time information
use shadow_rs::shadow;
shadow!(build);

// Create enhanced version information at compile time
const ENHANCED_VERSION: &str = shadow_rs::formatcp!(
    "version v{}\nBuilt: {}\nCommit: {}\nRust version: {}\nPlatform: {}",
    build::PKG_VERSION,
    build::BUILD_TIME_3339,
    build::SHORT_COMMIT,
    build::RUST_VERSION,
    build::BUILD_TARGET
);

// GraphQL returns UPPERCASE enum values but octocrab expects snake_case
// variants.
fn deserialize_graphql_conclusion<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<Conclusion>, D::Error>
where
    D: Deserializer<'de>,
{
    let s = Option::<String>::deserialize(deserializer)?;
    let conclusion = s.as_ref().map(|s| match s.as_str() {
        "SUCCESS" => Conclusion::Success,
        "FAILURE" => Conclusion::Failure,
        "CANCELLED" => Conclusion::Cancelled,
        "TIMED_OUT" => Conclusion::TimedOut,
        "ACTION_REQUIRED" => Conclusion::ActionRequired,
        "NEUTRAL" => Conclusion::Neutral,
        "SKIPPED" => Conclusion::Skipped,
        unknown => panic!("Unknown GraphQL conclusion value: '{}'", unknown),
    });
    Ok(conclusion)
}

// GraphQL status states are lowercase strings but octocrab uses Pascal case
// enums.
fn deserialize_graphql_status_state<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<StatusState>, D::Error>
where
    D: Deserializer<'de>,
{
    let s = Option::<String>::deserialize(deserializer)?;
    let state = s.as_ref().map(|s| match s.to_lowercase().as_str() {
        "success" => StatusState::Success,
        "failure" => StatusState::Failure,
        "pending" => StatusState::Pending,
        "error" => StatusState::Error,
        unknown => panic!("Unknown GraphQL status state value: '{}'", unknown),
    });
    Ok(state)
}

/// Represents the overall CI status of a pull request across all checks.
#[derive(Debug, Clone, PartialEq)]
pub enum CiStatus {
    Success,
    Failing,
    Pending,
    Unknown,
}

/// Well-known GitHub labels used for PR workflow automation.
#[derive(Debug, Clone, PartialEq)]
pub enum KnownLabel {
    Approved,
    Lgtm,
    NeedsOkToTest,
    DoNotMergeHold,
    OkToTest,
}

/// Bot commands that can be posted as PR comments to trigger actions.
#[derive(Debug, Clone, PartialEq)]
pub enum BotCommand {
    Approve,
    Lgtm,
    OkToTest,
}

/// Distinguishes between different types of GitHub status check contexts.
#[derive(Debug, Clone, PartialEq)]
pub enum GraphQLContextType {
    CheckRun,
    StatusContext,
    Unknown,
}

/// Categorises different types of GitHub actors for author filtering.
#[derive(Debug, Clone, PartialEq)]
pub enum AuthorType {
    User,
    Bot,
    App,
    Unknown,
}

/// Pull request states for GitHub search queries.
#[derive(Debug, Clone, PartialEq)]
pub enum SearchState {
    Open,
    Closed,
}

/// Status check states for GitHub search filtering.
#[derive(Debug, Clone, PartialEq)]
pub enum SearchStatus {
    Success,
    Failure,
    Pending,
}

/// Common error patterns found in CI build logs.
#[derive(Debug, Clone, PartialEq)]
pub enum LogErrorPattern {
    Error,
    Failed,
    Failure,
    Fatal,
    Panic,
    ErrorPrefix,
    FailPrefix,
    ExitCode,
}

impl KnownLabel {
    pub fn as_str(&self) -> &'static str {
        match self {
            KnownLabel::Approved => "approved",
            KnownLabel::Lgtm => "lgtm",
            KnownLabel::NeedsOkToTest => "needs-ok-to-test",
            KnownLabel::DoNotMergeHold => "do-not-merge/hold",
            KnownLabel::OkToTest => "ok-to-test",
        }
    }
}

impl BotCommand {
    pub fn as_str(&self) -> &'static str {
        match self {
            BotCommand::Approve => "/approve",
            BotCommand::Lgtm => "/lgtm",
            BotCommand::OkToTest => "/ok-to-test",
        }
    }
}

impl GraphQLContextType {
    pub fn from_typename(typename: &str) -> Self {
        match typename {
            "CheckRun" => GraphQLContextType::CheckRun,
            "StatusContext" => GraphQLContextType::StatusContext,
            _ => GraphQLContextType::Unknown,
        }
    }
}

impl AuthorType {
    pub fn from_typename(typename: &str) -> Self {
        match typename {
            "User" => AuthorType::User,
            "Bot" => AuthorType::Bot,
            "App" => AuthorType::App,
            _ => AuthorType::Unknown,
        }
    }

    pub fn is_bot(&self) -> bool {
        matches!(self, AuthorType::Bot | AuthorType::App)
    }
}

impl SearchState {
    pub fn as_str(&self) -> &'static str {
        match self {
            SearchState::Open => "open",
            SearchState::Closed => "closed",
        }
    }
}

impl SearchStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            SearchStatus::Success => "success",
            SearchStatus::Failure => "failure",
            SearchStatus::Pending => "pending",
        }
    }
}

impl LogErrorPattern {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogErrorPattern::Error => "error:",
            LogErrorPattern::Failed => "failed:",
            LogErrorPattern::Failure => "failure:",
            LogErrorPattern::Fatal => "fatal:",
            LogErrorPattern::Panic => "panic:",
            LogErrorPattern::ErrorPrefix => "E ",
            LogErrorPattern::FailPrefix => "FAIL ",
            LogErrorPattern::ExitCode => "exit code",
        }
    }

    pub fn all_patterns() -> Vec<LogErrorPattern> {
        vec![
            LogErrorPattern::Error,
            LogErrorPattern::Failed,
            LogErrorPattern::Failure,
            LogErrorPattern::Fatal,
            LogErrorPattern::Panic,
            LogErrorPattern::ErrorPrefix,
            LogErrorPattern::FailPrefix,
            LogErrorPattern::ExitCode,
        ]
    }

    pub fn matches(&self, line: &str) -> bool {
        let pattern = self.as_str();
        if pattern.ends_with(' ') {
            line.starts_with(pattern)
        } else {
            line.to_lowercase().contains(pattern)
        }
    }
}

impl CiStatus {
    fn from_conclusion(conclusion: &Conclusion) -> Result<Self> {
        match conclusion {
            Conclusion::Success => Ok(CiStatus::Success),
            Conclusion::Failure | Conclusion::Cancelled | Conclusion::TimedOut => {
                Ok(CiStatus::Failing)
            }
            Conclusion::ActionRequired | Conclusion::Neutral | Conclusion::Skipped => {
                Ok(CiStatus::Pending)
            }
            unknown => anyhow::bail!("Unknown Conclusion variant encountered: {:?}", unknown),
        }
    }

    fn from_status_state(state: &StatusState) -> Result<Self> {
        match state {
            StatusState::Success => Ok(CiStatus::Success),
            StatusState::Failure | StatusState::Error => Ok(CiStatus::Failing),
            StatusState::Pending => Ok(CiStatus::Pending),
            unknown => anyhow::bail!("Unknown StatusState variant encountered: {:?}", unknown),
        }
    }
}

impl std::fmt::Display for CiStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CiStatus::Success => write!(f, "Success"),
            CiStatus::Failing => write!(f, "Failing"),
            CiStatus::Pending => write!(f, "Pending"),
            CiStatus::Unknown => write!(f, "Unknown"),
        }
    }
}

#[derive(Parser)]
#[command(name = "autoprat")]
#[command(
    about = "Stop clicking through GitHub PRs one by one - finds PRs you care about and generates commands to act on them in bulk"
)]
#[command(version, long_version = ENHANCED_VERSION)]
struct Cli {
    /// GitHub repository in format 'owner/repo' (required when using numeric PR
    /// arguments or no PR arguments)
    #[arg(short = 'r', long = "repo")]
    repo: Option<String>,

    /// PR numbers or URLs to focus on (can specify multiple)
    #[arg(help = "PR-NUMBER|PR-URL ...")]
    prs: Vec<String>,

    /// Exact author match
    #[arg(short = 'a', long = "author")]
    author: Option<String>,

    /// Has label (prefix ! to negate)
    #[arg(long)]
    label: Vec<String>,

    /// Has failing CI checks
    #[arg(long = "failing-ci")]
    failing_ci: bool,

    /// Specific CI check is failing (exact match)
    #[arg(long = "failing-check")]
    failing_check: Vec<String>,

    /// Missing 'approved' label
    #[arg(long = "needs-approve")]
    needs_approve: bool,

    /// Missing 'lgtm' label
    #[arg(long = "needs-lgtm")]
    needs_lgtm: bool,

    /// Has 'needs-ok-to-test' label
    #[arg(long = "needs-ok-to-test")]
    needs_ok_to_test: bool,

    /// Generate /approve commands
    #[arg(long)]
    approve: bool,

    /// Generate /lgtm commands
    #[arg(long)]
    lgtm: bool,

    /// Generate /ok-to-test commands
    #[arg(long = "ok-to-test")]
    ok_to_test: bool,

    /// Generate custom comment commands
    #[arg(long)]
    comment: Option<String>,

    /// Skip if same comment posted recently (e.g. 5m, 1h)
    #[arg(long)]
    throttle: Option<String>,

    /// Show detailed PR information
    #[arg(short = 'd', long = "detailed")]
    detailed: bool,

    /// Show detailed PR information with error logs from failing checks
    #[arg(short = 'D', long = "detailed-with-logs")]
    detailed_with_logs: bool,

    /// Print PR numbers only
    #[arg(short = 'q', long = "quiet")]
    quiet: bool,

    /// Enable debug logging
    #[arg(long)]
    debug: bool,

    /// Limit the number of PRs to process
    #[arg(short = 'L', long, default_value_t = 30)]
    limit: usize,
}

#[derive(Debug)]
pub enum PipelineError {
    FormatError(String),
    GitHubError(String),
    ConfigError(String),
}

impl std::fmt::Display for PipelineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PipelineError::FormatError(msg) => write!(f, "Format error: {}", msg),
            PipelineError::GitHubError(msg) => write!(f, "GitHub error: {}", msg),
            PipelineError::ConfigError(msg) => write!(f, "Config error: {}", msg),
        }
    }
}

impl std::error::Error for PipelineError {}

/// Information about a single CI check or status from GitHub.
#[derive(Debug)]
struct CheckInfo {
    name: String,
    conclusion: Option<Conclusion>,
    status_state: Option<StatusState>,
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GraphQLResponse {
    data: SearchData,
}

#[derive(Debug, Deserialize)]
struct SearchData {
    search: SearchResults,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchResults {
    nodes: Vec<GraphQLPullRequest>,
    page_info: PageInfo,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageInfo {
    has_next_page: bool,
    end_cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GraphQLPullRequest {
    number: u64,
    title: String,
    url: String,
    created_at: DateTime<Utc>,
    author: Option<GraphQLAuthor>,
    labels: GraphQLLabelConnection,
    status_check_rollup: Option<GraphQLStatusCheckRollup>,
    comments: GraphQLCommentConnection,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GraphQLAuthor {
    login: String,
    #[serde(rename = "__typename")]
    typename: String,
}

impl GraphQLAuthor {
    /// Returns the properly formatted author string for GitHub search.
    /// For bots (App or Bot), returns "app/login", for users returns just
    /// "login".
    fn search_format(&self) -> String {
        let author_type = AuthorType::from_typename(&self.typename);
        if author_type.is_bot() {
            format!("app/{}", self.login)
        } else {
            self.login.clone()
        }
    }

    /// Returns the display format with bot indicator for UI display
    fn display_format(&self) -> String {
        let author_type = AuthorType::from_typename(&self.typename);
        if author_type.is_bot() {
            format!("{}[bot]", self.login)
        } else {
            self.login.clone()
        }
    }

    /// Returns just the login name for simplified display
    fn simple_name(&self) -> String {
        self.login.clone()
    }
}

#[derive(Debug, Deserialize)]
struct GraphQLLabelConnection {
    nodes: Vec<GraphQLLabel>,
}

#[derive(Debug, Deserialize)]
struct GraphQLLabel {
    name: String,
}

#[derive(Debug, Deserialize)]
struct GraphQLStatusCheckRollup {
    contexts: GraphQLStatusContextConnection,
}

#[derive(Debug, Deserialize)]
struct GraphQLStatusContextConnection {
    nodes: Vec<GraphQLStatusContext>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GraphQLStatusContext {
    #[serde(rename = "__typename")]
    typename: String,
    // CheckRun fields
    name: Option<String>,
    #[serde(deserialize_with = "deserialize_graphql_conclusion", default)]
    conclusion: Option<Conclusion>,
    details_url: Option<String>,
    // StatusContext fields
    context: Option<String>,
    #[serde(deserialize_with = "deserialize_graphql_status_state", default)]
    state: Option<StatusState>,
    target_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GraphQLCommentConnection {
    nodes: Vec<GraphQLComment>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GraphQLComment {
    body: String,
    created_at: DateTime<Utc>,
}

#[derive(Debug)]
struct SearchQueryBuilder {
    terms: Vec<String>,
}

impl SearchQueryBuilder {
    fn new() -> Self {
        Self { terms: Vec::new() }
    }

    fn repo(&mut self, owner: &str, name: &str) -> &mut Self {
        self.terms.push(format!("repo:{}/{}", owner, name));
        self
    }

    fn pr_type(&mut self) -> &mut Self {
        self.terms.push("type:pr".to_string());
        self
    }

    fn state(&mut self, state: SearchState) -> &mut Self {
        self.terms.push(format!("state:{}", state.as_str()));
        self
    }

    fn label(&mut self, label: &str) -> &mut Self {
        self.terms.push(format!("label:{}", label));
        self
    }

    fn no_label(&mut self, label: &str) -> &mut Self {
        self.terms.push(format!("-label:{}", label));
        self
    }

    fn status(&mut self, status: SearchStatus) -> &mut Self {
        self.terms.push(format!("status:{}", status.as_str()));
        self
    }

    fn build(&self) -> String {
        self.terms.join(" ")
    }
}

/// Information about a pull request comment for throttling analysis.
#[derive(Debug)]
struct CommentInfo {
    body: String,
    created_at: DateTime<Utc>,
}

/// Essential pull request information extracted from GitHub's GraphQL response.
#[derive(Debug)]
struct SimplePR {
    number: u64,
    title: String,
    author_login: String,
    author_search_format: String,
    author_simple_name: String,
    url: String,
    labels: Vec<String>,
    created_at: DateTime<Utc>,
}

/// Complete pull request information including repository context and
/// associated data.
#[derive(Debug)]
struct PrInfo {
    repo_owner: String,
    repo_name: String,
    pr: SimplePR,
    checks: Vec<CheckInfo>,
    recent_comments: Vec<CommentInfo>,
}

fn get_github_token() -> Result<String> {
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

fn parse_repo_from_string(repo: &str) -> Result<(&str, &str)> {
    let parts: Vec<&str> = repo.split('/').collect();
    if parts.len() != 2 {
        anyhow::bail!("Repository must be in format 'owner/repo', got: '{}'", repo);
    }
    Ok((parts[0], parts[1]))
}

fn parse_pr_url(url_str: &str) -> Result<(String, String, u64)> {
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

fn parse_duration(duration_str: &str) -> Result<chrono::Duration> {
    let duration_str = duration_str.trim();

    if let Some(num_str) = duration_str.strip_suffix('m') {
        let minutes: i64 = num_str
            .parse()
            .with_context(|| format!("Invalid minutes value in duration: '{}'", duration_str))?;
        Ok(chrono::Duration::minutes(minutes))
    } else if let Some(num_str) = duration_str.strip_suffix('h') {
        let hours: i64 = num_str
            .parse()
            .with_context(|| format!("Invalid hours value in duration: '{}'", duration_str))?;
        Ok(chrono::Duration::hours(hours))
    } else if let Some(num_str) = duration_str.strip_suffix('s') {
        let seconds: i64 = num_str
            .parse()
            .with_context(|| format!("Invalid seconds value in duration: '{}'", duration_str))?;
        Ok(chrono::Duration::seconds(seconds))
    } else {
        // Try parsing as minutes if no suffix
        let minutes: i64 = duration_str.parse().with_context(|| {
            format!(
                "Invalid duration (expected format: 5m, 2h, 30s): '{}'",
                duration_str
            )
        })?;
        Ok(chrono::Duration::minutes(minutes))
    }
}

fn format_relative_time(created_at: DateTime<Utc>) -> String {
    let now = Utc::now();
    let duration = now.signed_duration_since(created_at);

    if duration.num_days() > 0 {
        let days = duration.num_days();
        if days == 1 {
            "about 1 day ago".to_string()
        } else {
            format!("about {} days ago", days)
        }
    } else if duration.num_hours() > 0 {
        let hours = duration.num_hours();
        if hours == 1 {
            "about 1 hour ago".to_string()
        } else {
            format!("about {} hours ago", hours)
        }
    } else if duration.num_minutes() > 0 {
        let minutes = duration.num_minutes();
        if minutes == 1 {
            "about 1 minute ago".to_string()
        } else {
            format!("about {} minutes ago", minutes)
        }
    } else {
        "about a minute ago".to_string()
    }
}

fn create_graphql_query() -> serde_json::Value {
    serde_json::json!({
        "query": r#"
            query($query: String!, $after: String) {
                search(query: $query, type: ISSUE, first: 100, after: $after) {
                    nodes {
                        ... on PullRequest {
                            number
                            title
                            url
                            state
                            createdAt
                            author {
                                login
                                __typename
                            }
                            labels(first: 20) {
                                nodes {
                                    name
                                }
                            }
                            statusCheckRollup {
                                contexts(first: 100) {
                                    nodes {
                                        __typename
                                        ... on CheckRun {
                                            name
                                            conclusion
                                            detailsUrl
                                        }
                                        ... on StatusContext {
                                            context
                                            state
                                            targetUrl
                                        }
                                    }
                                }
                            }
                            comments(last: 15) {
                                nodes {
                                    body
                                    createdAt
                                    author {
                                        login
                                        __typename
                                    }
                                }
                            }
                        }
                    }
                    pageInfo {
                        hasNextPage
                        endCursor
                    }
                }
            }
        "#,
        "variables": {}
    })
}

// Streaming processor that accumulates results from all pages before displaying
async fn process_prs_streaming(
    octocrab: &Octocrab,
    search_query: &str,
    cli: &Cli,
) -> Result<(Vec<String>, Vec<PrInfo>)> {
    let mut action_commands = Vec::new();
    let mut all_prs = Vec::new();
    let has_actions = cli.approve || cli.lgtm || cli.ok_to_test || cli.comment.is_some();
    let mut after_cursor: Option<String> = None;
    let mut page_count = 0;
    let mut processed_count = 0;

    loop {
        page_count += 1;
        let mut query = create_graphql_query();
        query["variables"]["query"] = serde_json::Value::String(search_query.to_string());

        // Add cursor for pagination if we have one
        if let Some(cursor) = &after_cursor {
            query["variables"]["after"] = serde_json::Value::String(cursor.clone());
        } else {
            query["variables"]["after"] = serde_json::Value::Null;
        }

        let response: GraphQLResponse = octocrab.graphql(&query).await?;
        let search_results = response.data.search;

        // Process this page's PRs immediately
        let mut page_prs = Vec::new();
        for graphql_pr in search_results.nodes {
            if let Ok(pr_info) = convert_graphql_pr_to_pr_info(graphql_pr) {
                page_prs.push(pr_info);
            }
        }

        // Apply filters to this page
        let filtered_page_prs = apply_remaining_filters(page_prs, cli);

        // Accumulate results from this page
        for pr_info in filtered_page_prs {
            // Check if we've reached the limit
            if processed_count >= cli.limit {
                return Ok((action_commands, all_prs));
            }

            // Always accumulate PRs for later display
            all_prs.push(pr_info);

            if has_actions {
                // Generate action commands using the accumulated PR
                let commands = generate_action_commands(&all_prs[all_prs.len() - 1..], cli)?;
                action_commands.extend(commands);
            }
            processed_count += 1;
        }

        // Check if there are more pages
        if !search_results.page_info.has_next_page {
            break;
        }

        // Set up cursor for next page
        after_cursor = search_results.page_info.end_cursor;

        // Safety check to prevent infinite loops
        if after_cursor.is_none() {
            break;
        }

        // Safety limit to prevent too many requests
        if page_count >= 20 {
            break;
        }
    }

    Ok((action_commands, all_prs))
}

/// Extracts repository owner and name from a GitHub PR URL.
fn extract_repo_info_from_url(url: &str) -> Result<(String, String)> {
    let url_parts: Vec<&str> = url.split('/').collect();
    if url_parts.len() < 5 {
        anyhow::bail!("Invalid PR URL format: '{}'", url);
    }
    Ok((url_parts[3].to_string(), url_parts[4].to_string()))
}

/// Converts a GraphQL status context into a unified CheckInfo structure.
fn convert_graphql_status_context(context: GraphQLStatusContext) -> CheckInfo {
    match GraphQLContextType::from_typename(&context.typename) {
        GraphQLContextType::CheckRun => CheckInfo {
            name: context.name.unwrap_or_else(|| "Unknown Check".to_string()),
            conclusion: context.conclusion,
            status_state: None,
            url: context.details_url,
        },
        GraphQLContextType::StatusContext => CheckInfo {
            name: context
                .context
                .unwrap_or_else(|| "Unknown Status".to_string()),
            conclusion: None,
            status_state: context.state,
            url: context.target_url,
        },
        GraphQLContextType::Unknown => CheckInfo {
            name: "Unknown".to_string(),
            conclusion: None,
            status_state: None,
            url: None,
        },
    }
}

/// Converts an optional GraphQL status check rollup into a vector of CheckInfo.
fn convert_status_checks(rollup: Option<GraphQLStatusCheckRollup>) -> Vec<CheckInfo> {
    rollup.map_or_else(Vec::new, |rollup| {
        rollup
            .contexts
            .nodes
            .into_iter()
            .map(convert_graphql_status_context)
            .collect()
    })
}

/// Converts GraphQL comment connection into a vector of CommentInfo for
/// analysis.
fn convert_comments(comments: GraphQLCommentConnection) -> Vec<CommentInfo> {
    comments
        .nodes
        .into_iter()
        .map(|comment| CommentInfo {
            body: comment.body,
            created_at: comment.created_at,
        })
        .collect()
}

/// Converts a GraphQL pull request response into a complete PrInfo structure.
fn convert_graphql_pr_to_pr_info(graphql_pr: GraphQLPullRequest) -> Result<PrInfo> {
    let (repo_owner, repo_name) = extract_repo_info_from_url(&graphql_pr.url)?;

    let checks = convert_status_checks(graphql_pr.status_check_rollup);
    let recent_comments = convert_comments(graphql_pr.comments);

    let pr = SimplePR {
        number: graphql_pr.number,
        title: graphql_pr.title,
        author_login: graphql_pr
            .author
            .as_ref()
            .map(|a| a.display_format())
            .unwrap_or_else(|| "Unknown".to_string()),
        author_search_format: graphql_pr
            .author
            .as_ref()
            .map(|a| a.search_format())
            .unwrap_or_else(|| "Unknown".to_string()),
        author_simple_name: graphql_pr
            .author
            .map(|a| a.simple_name())
            .unwrap_or_else(|| "Unknown".to_string()),
        url: graphql_pr.url,
        labels: graphql_pr
            .labels
            .nodes
            .into_iter()
            .map(|label| label.name)
            .collect(),
        created_at: graphql_pr.created_at,
    };

    Ok(PrInfo {
        repo_owner,
        repo_name,
        pr,
        checks,
        recent_comments,
    })
}

fn has_label(pr: &SimplePR, known_label: KnownLabel) -> bool {
    pr.labels.iter().any(|label| label == known_label.as_str())
}

fn has_specific_failing_check(checks: &[CheckInfo], check_name: &str) -> bool {
    checks.iter().any(|check| {
        if check.name != check_name {
            return false;
        }

        if let Some(Conclusion::Failure | Conclusion::Cancelled | Conclusion::TimedOut) =
            &check.conclusion
        {
            return true;
        }

        if let Some(StatusState::Failure | StatusState::Error) = &check.status_state {
            return true;
        }

        false
    })
}

/// Tests if a PR matches the author filter with intelligent bot account
/// handling.
fn matches_author_filter(pr: &SimplePR, author: &str) -> bool {
    pr.author_login == author
        || pr.author_search_format == author
        || (pr.author_login.starts_with(&format!("{}[", author)) && pr.author_login.ends_with("]"))
        || (pr.author_search_format == format!("app/{}", author))
}

/// Tests if a PR matches label-based filtering requirements.
fn matches_label_filter(pr: &SimplePR, cli: &Cli) -> bool {
    !cli.needs_ok_to_test || has_label(pr, KnownLabel::NeedsOkToTest)
}

/// Tests if a PR has all specified failing checks.
fn matches_failing_check_filters(pr_info: &PrInfo, failing_checks: &[String]) -> bool {
    failing_checks
        .iter()
        .all(|check_name| has_specific_failing_check(&pr_info.checks, check_name))
}

fn apply_remaining_filters(prs: Vec<PrInfo>, cli: &Cli) -> Vec<PrInfo> {
    prs.into_iter()
        .filter(|pr_info| {
            let pr = &pr_info.pr;

            if let Some(author) = &cli.author {
                if !matches_author_filter(pr, author) {
                    return false;
                }
            }

            if !matches_label_filter(pr, cli) {
                return false;
            }

            if !matches_failing_check_filters(pr_info, &cli.failing_check) {
                return false;
            }

            true
        })
        .collect()
}

fn should_throttle_comment(
    comment_text: &str,
    recent_comments: &[CommentInfo],
    throttle_duration: Option<chrono::Duration>,
) -> bool {
    if let Some(duration) = throttle_duration {
        let now = Utc::now();
        let cutoff_time = now - duration;

        for comment in recent_comments {
            if comment.created_at > cutoff_time && comment.body.trim() == comment_text.trim() {
                return true;
            }
        }
    }

    false
}

/// Predicate function that determines if an action should be applied to a PR.
type ActionCondition = fn(&SimplePR) -> bool;

/// Specification for a bot action: enabled flag, command, and application
/// condition.
type ActionSpec = (bool, BotCommand, ActionCondition);

fn generate_action_commands(prs: &[PrInfo], cli: &Cli) -> Result<Vec<String>> {
    let mut commands = Vec::new();

    let throttle_duration = if let Some(throttle_str) = &cli.throttle {
        Some(
            parse_duration(throttle_str)
                .with_context(|| format!("Invalid throttle duration: '{}'", throttle_str))?,
        )
    } else {
        None
    };

    let actions_to_perform: &[ActionSpec] = &[
        (cli.approve, BotCommand::Approve, |pr: &SimplePR| {
            !has_label(pr, KnownLabel::Approved)
        }),
        (cli.lgtm, BotCommand::Lgtm, |pr: &SimplePR| {
            !has_label(pr, KnownLabel::Lgtm)
        }),
        (cli.ok_to_test, BotCommand::OkToTest, |pr: &SimplePR| {
            has_label(pr, KnownLabel::NeedsOkToTest)
        }),
    ];

    for pr_info in prs {
        let repo_full = format!("{}/{}", pr_info.repo_owner, pr_info.repo_name);
        let pr_number = pr_info.pr.number;

        for (enabled, command, condition) in actions_to_perform {
            if *enabled
                && condition(&pr_info.pr)
                && !should_throttle_comment(
                    command.as_str(),
                    &pr_info.recent_comments,
                    throttle_duration,
                )
            {
                commands.push(format!(
                    "gh pr comment {} --repo {} --body \"{}\"",
                    pr_number,
                    repo_full,
                    command.as_str()
                ));
            }
        }
        if let Some(comment_text) = &cli.comment {
            if !should_throttle_comment(comment_text, &pr_info.recent_comments, throttle_duration) {
                commands.push(format!(
                    "gh pr comment {} --repo {} --body \"{}\"",
                    pr_number, repo_full, comment_text
                ));
            }
        }
    }

    Ok(commands)
}

fn fetch_and_filter_logs(url: &str) -> Result<Vec<String>> {
    // Convert Prow CI URLs to direct log URLs
    let log_url = if url.contains("prow.ci.openshift.org/view/gs/") {
        url.replace("prow.ci.openshift.org/view/gs/", "storage.googleapis.com/") + "/build-log.txt"
    } else if url.contains("raw") || url.contains("storage.googleapis.com") {
        // Already a raw log URL
        url.to_string()
    } else {
        // Skip non-log URLs (e.g., GitHub comment URLs)
        if url.contains("#issuecomment") {
            return Ok(Vec::new());
        }
        // For other CI systems, we might not know how to get logs
        return Ok(Vec::new());
    };

    // Fetch the log content
    let response = match ureq::get(&log_url)
        .timeout(std::time::Duration::from_secs(10))
        .call()
    {
        Ok(resp) => resp,
        Err(_) => return Ok(Vec::new()),
    };

    if response.status() != 200 {
        return Ok(Vec::new());
    }

    let content = match response.into_string() {
        Ok(text) => text,
        Err(_) => return Ok(Vec::new()),
    };

    filter_error_lines_from_content(&content)
}

fn filter_error_lines_from_content(content: &str) -> Result<Vec<String>> {
    static ERROR_PATTERNS: std::sync::OnceLock<Vec<LogErrorPattern>> = std::sync::OnceLock::new();
    let error_patterns = ERROR_PATTERNS.get_or_init(LogErrorPattern::all_patterns);

    let mut error_lines = Vec::new();
    for line in content.lines() {
        // Skip empty lines and very long lines
        if line.trim().is_empty() || line.len() > 500 {
            continue;
        }

        let is_error = error_patterns.iter().any(|pattern| pattern.matches(line));

        // Also check for exit codes 1-9
        if is_error
            || (line.contains("exit code") && line.chars().any(|c| ('1'..='9').contains(&c)))
        {
            error_lines.push(line.trim().to_string());

            // Limit to 20 error lines
            if error_lines.len() >= 20 {
                error_lines.push("... (truncated)".to_string());
                break;
            }
        }
    }

    Ok(error_lines)
}

fn get_ci_status(checks: &[CheckInfo]) -> Result<CiStatus> {
    if checks.is_empty() {
        return Ok(CiStatus::Unknown);
    }

    let mut has_failure = false;
    let mut has_pending = false;
    let mut has_success = false;

    for check in checks {
        if let Some(conclusion) = &check.conclusion {
            match CiStatus::from_conclusion(conclusion)? {
                CiStatus::Success => has_success = true,
                CiStatus::Failing => has_failure = true,
                CiStatus::Pending => has_pending = true,
                CiStatus::Unknown => has_pending = true,
            }
        }

        if let Some(state) = &check.status_state {
            match CiStatus::from_status_state(state)? {
                CiStatus::Success => has_success = true,
                CiStatus::Failing => has_failure = true,
                CiStatus::Pending => has_pending = true,
                CiStatus::Unknown => has_pending = true,
            }
        }

        // If neither conclusion nor state is present, treat as pending
        if check.conclusion.is_none() && check.status_state.is_none() {
            has_pending = true;
        }
    }

    if has_failure {
        Ok(CiStatus::Failing)
    } else if has_pending {
        Ok(CiStatus::Pending)
    } else if has_success {
        Ok(CiStatus::Success)
    } else {
        Ok(CiStatus::Unknown)
    }
}

fn display_prs_table(prs: &[PrInfo]) -> Result<()> {
    if prs.is_empty() {
        println!("No pull requests found matching filters.");
        return Ok(());
    }

    let mut table = Table::new();
    table.load_preset(NOTHING);
    table.set_header(vec![
        "URL",
        "CI",
        "APP",
        "LGTM",
        "OK2TST",
        "HOLD",
        "AUTHOR",
        "TITLE",
        "CREATED AT",
    ]);

    for pr_info in prs {
        let pr = &pr_info.pr;
        let ci_status = get_ci_status(&pr_info.checks)?;
        let approved = if has_label(pr, KnownLabel::Approved) {
            "✓"
        } else {
            "✗"
        };
        let lgtm = if has_label(pr, KnownLabel::Lgtm) {
            "✓"
        } else {
            "✗"
        };
        let ok2test = if has_label(pr, KnownLabel::OkToTest) {
            "✓"
        } else {
            "✗"
        };
        let hold = if has_label(pr, KnownLabel::DoNotMergeHold) {
            "Y"
        } else {
            "N"
        };

        table.add_row(vec![
            Cell::new(&pr.url),
            Cell::new(ci_status.to_string()),
            Cell::new(approved),
            Cell::new(lgtm),
            Cell::new(ok2test),
            Cell::new(hold),
            Cell::new(&pr.author_simple_name),
            Cell::new(&pr.title),
            Cell::new(format_relative_time(pr.created_at)),
        ]);
    }

    println!("{}", table);
    Ok(())
}

fn display_prs_quiet(prs: &[PrInfo]) {
    for pr_info in prs {
        println!("{}", pr_info.pr.number);
    }
}

fn display_prs_verbose(prs: &[PrInfo], show_logs: bool) -> Result<()> {
    if prs.is_empty() {
        println!("No pull requests found matching filters.");
        return Ok(());
    }

    // Group PRs by repository
    let mut repos = std::collections::HashMap::new();
    for pr_info in prs {
        let repo_key = format!("{}/{}", pr_info.repo_owner, pr_info.repo_name);
        repos.entry(repo_key).or_insert_with(Vec::new).push(pr_info);
    }

    for (repo_name, repo_prs) in repos {
        println!("Repository: {}", repo_name);
        println!("=====================================");

        for pr_info in repo_prs {
            display_single_pr_tree(pr_info, show_logs)?;
        }
    }
    Ok(())
}

fn display_single_pr_tree(pr_info: &PrInfo, show_logs: bool) -> Result<()> {
    let pr = &pr_info.pr;

    // Main PR info
    println!("● {}", pr.url);
    println!("├─Title: {} ({})", pr.title, pr.author_login);
    println!("├─PR #{}", pr.number);
    println!("├─State: OPEN");
    println!("├─Created: {}", pr.created_at.format("%Y-%m-%dT%H:%M:%SZ"));

    // Status section
    println!("├─Status");
    println!(
        "│ ├─Approved: {}",
        if has_label(pr, KnownLabel::Approved) {
            "Yes"
        } else {
            "No"
        }
    );
    println!("│ ├─CI: {}", get_ci_status(&pr_info.checks)?);
    println!(
        "│ ├─LGTM: {}",
        if has_label(pr, KnownLabel::Lgtm) {
            "Yes"
        } else {
            "No"
        }
    );
    println!(
        "│ └─OK-to-test: {}",
        if has_label(pr, KnownLabel::OkToTest) {
            "Yes"
        } else {
            "No"
        }
    );

    // Labels section
    println!("├─Labels");
    if pr.labels.is_empty() {
        println!("│ └─None");
    } else {
        for (i, label) in pr.labels.iter().enumerate() {
            if i == pr.labels.len() - 1 {
                println!("│ └─{}", label);
            } else {
                println!("│ ├─{}", label);
            }
        }
    }

    // Checks section
    println!("└─Checks");
    if pr_info.checks.is_empty() {
        println!("  └─None");
    } else {
        display_checks_tree(&pr_info.checks, show_logs)?;
    }
    Ok(())
}

/// Determines the display status string for a check based on its conclusion or
/// state.
fn get_check_display_status(check: &CheckInfo) -> Result<&'static str> {
    if let Some(conclusion) = &check.conclusion {
        match conclusion {
            Conclusion::Success => Ok("SUCCESS"),
            Conclusion::Failure | Conclusion::Cancelled | Conclusion::TimedOut => Ok("FAILURE"),
            Conclusion::ActionRequired | Conclusion::Neutral | Conclusion::Skipped => Ok("PENDING"),
            unknown => anyhow::bail!("Unknown Conclusion variant in display: {:?}", unknown),
        }
    } else if let Some(state) = &check.status_state {
        match state {
            StatusState::Success => Ok("SUCCESS"),
            StatusState::Failure | StatusState::Error => Ok("FAILURE"),
            StatusState::Pending => Ok("PENDING"),
            unknown => anyhow::bail!("Unknown StatusState variant in display: {:?}", unknown),
        }
    } else {
        Ok("PENDING")
    }
}

/// Groups checks by their display status for organised tree output.
fn group_checks_by_status(
    checks: &[CheckInfo],
) -> Result<std::collections::HashMap<String, Vec<&CheckInfo>>> {
    let mut checks_by_status: std::collections::HashMap<String, Vec<&CheckInfo>> =
        std::collections::HashMap::new();
    for check in checks {
        let status = get_check_display_status(check)?;
        checks_by_status
            .entry(status.to_string())
            .or_default()
            .push(check);
    }
    Ok(checks_by_status)
}

/// Returns appropriate tree drawing prefixes based on position in the tree
/// structure.
fn get_tree_prefixes(
    is_last_group: bool,
    is_last_check: bool,
) -> (&'static str, &'static str, &'static str) {
    match (is_last_group, is_last_check) {
        (true, true) => ("    └─", "      └─", "      "),
        (true, false) => ("    ├─", "    │ └─", "    │ "),
        (false, true) => ("  │ └─", "  │   └─", "  │   "),
        (false, false) => ("  │ ├─", "  │ │ └─", "  │ │ "),
    }
}

/// Fetches and displays error logs from a failing check URL with appropriate
/// tree formatting.
fn display_check_logs(url: &str, log_prefix: &str) {
    if let Ok(logs) = fetch_and_filter_logs(url) {
        if !logs.is_empty() {
            println!("{}Error logs:", log_prefix);
            for log_line in logs {
                println!("{}{}", log_prefix, log_line);
            }
        }
    }
}

/// Displays a single check in the tree structure with optional log output.
fn display_individual_check(
    check: &CheckInfo,
    status: &str,
    is_last_group: bool,
    is_last_check: bool,
    show_logs: bool,
) {
    let (check_prefix, url_prefix, log_prefix) = get_tree_prefixes(is_last_group, is_last_check);

    println!("{}{}", check_prefix, check.name);

    if let Some(url) = &check.url {
        println!("{}URL: {}", url_prefix, url);
        if show_logs && status == "FAILURE" {
            display_check_logs(url, log_prefix);
        }
    }
}

/// Displays a group of checks with the same status in tree format.
fn display_status_group(status: &str, checks: &[&CheckInfo], is_last_group: bool, show_logs: bool) {
    let group_prefix = if is_last_group {
        "  └─"
    } else {
        "  ├─"
    };
    println!("{}{} ({})", group_prefix, status, checks.len());

    for (i, check) in checks.iter().enumerate() {
        let is_last_check = i == checks.len() - 1;
        display_individual_check(check, status, is_last_group, is_last_check, show_logs);
    }
}

/// Displays all checks in a hierarchical tree structure grouped by status.
fn display_checks_tree(checks: &[CheckInfo], show_logs: bool) -> Result<()> {
    const STATUS_ORDER: &[&str] = &["FAILURE", "PENDING", "SUCCESS", "UNKNOWN"];

    let checks_by_status = group_checks_by_status(checks)?;
    let mut displayed_groups = 0;
    let total_groups = checks_by_status.len();

    for status in STATUS_ORDER {
        if let Some(status_checks) = checks_by_status.get(*status) {
            displayed_groups += 1;
            let is_last_group = displayed_groups == total_groups;
            display_status_group(status, status_checks, is_last_group, show_logs);
        }
    }
    Ok(())
}

// Simple function to fetch a single PR by search query
async fn fetch_single_pr_by_query(
    octocrab: &Octocrab,
    search_query: &str,
) -> Result<Option<PrInfo>> {
    let mut query = create_graphql_query();
    query["variables"]["query"] = serde_json::Value::String(search_query.to_string());
    query["variables"]["after"] = serde_json::Value::Null;

    let response: GraphQLResponse = octocrab.graphql(&query).await?;

    if let Some(graphql_pr) = response.data.search.nodes.into_iter().next() {
        Ok(Some(convert_graphql_pr_to_pr_info(graphql_pr)?))
    } else {
        Ok(None)
    }
}

async fn collect_specific_prs(octocrab: &Octocrab, cli: &Cli) -> Result<Vec<PrInfo>> {
    let mut all_prs = Vec::new();

    for pr_arg in &cli.prs {
        if pr_arg.starts_with("https://github.com/") {
            // PR URL - extract info and use GraphQL search
            let (owner, repo_name, pr_number) = parse_pr_url(pr_arg)?;
            let search_query = format!("repo:{}/{} type:pr {}", owner, repo_name, pr_number);

            if let Some(pr_info) = fetch_single_pr_by_query(octocrab, &search_query).await? {
                if pr_info.pr.number == pr_number {
                    all_prs.push(pr_info);
                }
            }
        } else {
            // PR number - requires repo
            if let Some(repo) = &cli.repo {
                let (owner, repo_name) = parse_repo_from_string(repo)
                    .with_context(|| format!("Invalid repository format: '{}'", repo))?;
                let pr_number: u64 = pr_arg
                    .parse()
                    .with_context(|| format!("Invalid PR number: '{}'", pr_arg))?;
                let search_query = format!("repo:{}/{} type:pr {}", owner, repo_name, pr_number);

                if let Some(pr_info) = fetch_single_pr_by_query(octocrab, &search_query).await? {
                    if pr_info.pr.number == pr_number {
                        all_prs.push(pr_info);
                    }
                }
            } else {
                anyhow::bail!("Repository (--repo) is required when using PR numbers");
            }
        }
    }

    Ok(all_prs)
}

/// Creates an authenticated GitHub client using available credentials.
async fn setup_github_client() -> Result<Octocrab> {
    let token = get_github_token().context("Failed to obtain GitHub authentication token")?;
    Octocrab::builder()
        .personal_token(token)
        .build()
        .context("Failed to create GitHub client")
}

/// Constructs a GitHub search query string from CLI filters and repository
/// information.
fn build_search_query_from_cli(repo: &str, cli: &Cli) -> Result<String> {
    let (owner, repo_name) = parse_repo_from_string(repo)
        .with_context(|| format!("Invalid repository format: '{}'", repo))?;
    let mut query_builder = SearchQueryBuilder::new();
    query_builder
        .repo(owner, repo_name)
        .pr_type()
        .state(SearchState::Open);

    if cli.failing_ci {
        query_builder.status(SearchStatus::Failure);
    }

    for label in &cli.label {
        if let Some(label_name) = label.strip_prefix('!') {
            query_builder.no_label(label_name);
        } else {
            query_builder.label(label);
        }
    }

    if cli.needs_approve {
        query_builder.no_label(KnownLabel::Approved.as_str());
    }

    if cli.needs_lgtm {
        query_builder.no_label(KnownLabel::Lgtm.as_str());
    }

    Ok(query_builder.build())
}

/// Determines whether any action commands are enabled in the CLI configuration.
fn has_action_commands(cli: &Cli) -> bool {
    cli.approve || cli.lgtm || cli.ok_to_test || cli.comment.is_some()
}

/// Outputs generated GitHub CLI commands to stdout for execution.
fn output_commands(commands: Vec<String>) {
    for command in commands {
        println!("{}", command);
    }
}

/// Displays pull requests using the appropriate format based on CLI mode flags.
fn display_prs_by_mode(prs: &[PrInfo], cli: &Cli) -> Result<()> {
    if cli.quiet {
        display_prs_quiet(prs);
        Ok(())
    } else if cli.detailed || cli.detailed_with_logs {
        display_prs_verbose(prs, cli.detailed_with_logs)
    } else {
        display_prs_table(prs)
    }
}

/// Handles repository-wide PR search workflow using GitHub GraphQL streaming.
async fn handle_repository_search(octocrab: &Octocrab, cli: &Cli) -> Result<()> {
    let repo = cli
        .repo
        .as_ref()
        .context("Repository (--repo) is required when no PR arguments are specified")?;

    let search_query = build_search_query_from_cli(repo, cli)?;
    let (action_commands, all_prs) = process_prs_streaming(octocrab, &search_query, cli).await?;

    if has_action_commands(cli) {
        output_commands(action_commands);
    } else {
        display_prs_by_mode(&all_prs, cli)?;
    }

    Ok(())
}

/// Handles workflow for processing specific PRs provided via CLI arguments.
async fn handle_specific_prs(octocrab: &Octocrab, cli: &Cli) -> Result<()> {
    let all_prs = collect_specific_prs(octocrab, cli).await?;
    let filtered_prs = apply_remaining_filters(all_prs, cli);

    if has_action_commands(cli) {
        let commands = generate_action_commands(&filtered_prs, cli)?;
        output_commands(commands);
    } else {
        display_prs_by_mode(&filtered_prs, cli)?;
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let octocrab = setup_github_client().await?;

    if cli.prs.is_empty() {
        handle_repository_search(&octocrab, &cli).await
    } else {
        handle_specific_prs(&octocrab, &cli).await
    }
}
