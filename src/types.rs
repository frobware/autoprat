use chrono::{DateTime, Utc};
use octocrab::models::{StatusState, workflows::Conclusion};
use serde::{Deserialize, Deserializer};

// GraphQL returns UPPERCASE enum values but octocrab expects snake_case
// variants.
pub fn deserialize_graphql_conclusion<'de, D>(
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
pub fn deserialize_graphql_status_state<'de, D>(
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
    Retest,
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
    #[allow(dead_code)]
    Closed,
}

/// Status check states for GitHub search filtering.
#[derive(Debug, Clone, PartialEq)]
pub enum SearchStatus {
    #[allow(dead_code)]
    Success,
    Failure,
    #[allow(dead_code)]
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
            BotCommand::Retest => "/retest",
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
    pub fn from_conclusion(conclusion: &Conclusion) -> anyhow::Result<Self> {
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

    pub fn from_status_state(state: &StatusState) -> anyhow::Result<Self> {
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

/// Information about a single CI check or status from GitHub.
#[derive(Debug)]
pub struct CheckInfo {
    pub name: String,
    pub conclusion: Option<Conclusion>,
    pub status_state: Option<StatusState>,
    pub url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GraphQLResponse {
    pub data: SearchData,
}

#[derive(Debug, Deserialize)]
pub struct SearchData {
    pub search: SearchResults,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResults {
    pub nodes: Vec<GraphQLPullRequest>,
    pub page_info: PageInfo,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageInfo {
    pub has_next_page: bool,
    pub end_cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphQLPullRequest {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub created_at: DateTime<Utc>,
    pub author: Option<GraphQLAuthor>,
    pub labels: GraphQLLabelConnection,
    pub status_check_rollup: Option<GraphQLStatusCheckRollup>,
    pub comments: GraphQLCommentConnection,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphQLAuthor {
    pub login: String,
    #[serde(rename = "__typename")]
    pub typename: String,
}

impl GraphQLAuthor {
    /// Returns the properly formatted author string for GitHub search.
    /// For bots (App or Bot), returns "app/login", for users returns just
    /// "login".
    pub fn search_format(&self) -> String {
        let author_type = AuthorType::from_typename(&self.typename);
        if author_type.is_bot() {
            format!("app/{}", self.login)
        } else {
            self.login.clone()
        }
    }

    /// Returns the display format with bot indicator for UI display
    pub fn display_format(&self) -> String {
        let author_type = AuthorType::from_typename(&self.typename);
        if author_type.is_bot() {
            format!("{}[bot]", self.login)
        } else {
            self.login.clone()
        }
    }

    /// Returns just the login name for simplified display
    pub fn simple_name(&self) -> String {
        self.login.clone()
    }
}

#[derive(Debug, Deserialize)]
pub struct GraphQLLabelConnection {
    pub nodes: Vec<GraphQLLabel>,
}

#[derive(Debug, Deserialize)]
pub struct GraphQLLabel {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct GraphQLStatusCheckRollup {
    pub contexts: GraphQLStatusContextConnection,
}

#[derive(Debug, Deserialize)]
pub struct GraphQLStatusContextConnection {
    pub nodes: Vec<GraphQLStatusContext>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphQLStatusContext {
    #[serde(rename = "__typename")]
    pub typename: String,
    // CheckRun fields
    pub name: Option<String>,
    #[serde(deserialize_with = "deserialize_graphql_conclusion", default)]
    pub conclusion: Option<Conclusion>,
    pub details_url: Option<String>,
    // StatusContext fields
    pub context: Option<String>,
    #[serde(deserialize_with = "deserialize_graphql_status_state", default)]
    pub state: Option<StatusState>,
    pub target_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GraphQLCommentConnection {
    pub nodes: Vec<GraphQLComment>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphQLComment {
    pub body: String,
    pub created_at: DateTime<Utc>,
}

/// Information about a pull request comment for throttling analysis.
#[derive(Debug)]
pub struct CommentInfo {
    pub body: String,
    pub created_at: DateTime<Utc>,
}

/// Essential pull request information extracted from GitHub's GraphQL response.
#[derive(Debug)]
pub struct SimplePR {
    pub number: u64,
    pub title: String,
    pub author_login: String,
    pub author_search_format: String,
    pub author_simple_name: String,
    pub url: String,
    pub labels: Vec<String>,
    pub created_at: DateTime<Utc>,
}

/// Complete pull request information including repository context and
/// associated data.
#[derive(Debug)]
pub struct PrInfo {
    pub repo_owner: String,
    pub repo_name: String,
    pub pr: SimplePR,
    pub checks: Vec<CheckInfo>,
    pub recent_comments: Vec<CommentInfo>,
}
