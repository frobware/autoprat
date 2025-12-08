use std::collections::HashMap;

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use octocrab::{
    Octocrab,
    models::{StatusState, workflows::Conclusion},
};
use serde::{Deserialize, Deserializer};
use tracing::{debug, error, info, instrument, warn};
use url::Url;

use crate::types::{
    CheckConclusion, CheckInfo, CheckName, CheckRunStatus, CheckState, CheckUrl, CommentInfo,
    PullRequest, Repo,
};

#[derive(Debug, Deserialize)]
struct RateLimitInfo {
    limit: u32,
    remaining: u32,
    reset: u64,
    used: u32,
}

#[derive(Debug, Deserialize)]
struct RateLimitResources {
    core: RateLimitInfo,
    search: RateLimitInfo,
    graphql: RateLimitInfo,
}

#[derive(Debug, Deserialize)]
struct RateLimitResponse {
    resources: RateLimitResources,
}

/// Checks GitHub API rate limit status and logs the results.
///
/// Queries all rate limit categories (core, search, GraphQL) and warns
/// when limits are low. Returns core rate limit info for compatibility.
#[instrument(skip(octocrab), target = "autoprat::rate_limit")]
async fn check_rate_limit(octocrab: &Octocrab, context: &str) -> Result<RateLimitInfo> {
    debug!(target: "autoprat::rate_limit", "Checking GitHub API rate limit");

    let rate_limit: RateLimitResponse =
        octocrab
            .get("/rate_limit", None::<&()>)
            .await
            .map_err(|e| {
                warn!(
                    target: "autoprat::rate_limit",
                    context = context,
                    error = %e,
                    "Failed to check rate limit"
                );
                anyhow::anyhow!("Rate limit check failed: {e}")
            })?;

    let core_reset_time =
        chrono::DateTime::from_timestamp(rate_limit.resources.core.reset as i64, 0)
            .map(|dt| dt.format("%H:%M:%S UTC").to_string())
            .unwrap_or_else(|| "unknown".to_string());

    let graphql_reset_time =
        chrono::DateTime::from_timestamp(rate_limit.resources.graphql.reset as i64, 0)
            .map(|dt| dt.format("%H:%M:%S UTC").to_string())
            .unwrap_or_else(|| "unknown".to_string());

    let search_reset_time =
        chrono::DateTime::from_timestamp(rate_limit.resources.search.reset as i64, 0)
            .map(|dt| dt.format("%H:%M:%S UTC").to_string())
            .unwrap_or_else(|| "unknown".to_string());

    info!(
        target: "autoprat::rate_limit",
        context = context,
        api_type = "core",
        limit = rate_limit.resources.core.limit,
        remaining = rate_limit.resources.core.remaining,
        used = rate_limit.resources.core.used,
        reset_time = core_reset_time,
        "GitHub API rate limit status"
    );

    info!(
        target: "autoprat::rate_limit",
        context = context,
        api_type = "graphql",
        limit = rate_limit.resources.graphql.limit,
        remaining = rate_limit.resources.graphql.remaining,
        used = rate_limit.resources.graphql.used,
        reset_time = graphql_reset_time,
        "GitHub API rate limit status"
    );

    info!(
        target: "autoprat::rate_limit",
        context = context,
        api_type = "search",
        limit = rate_limit.resources.search.limit,
        remaining = rate_limit.resources.search.remaining,
        used = rate_limit.resources.search.used,
        reset_time = search_reset_time,
        "GitHub API rate limit status"
    );

    if rate_limit.resources.core.remaining < 10 {
        warn!(
            target: "autoprat::rate_limit",
            api_type = "core",
            remaining = rate_limit.resources.core.remaining,
            reset_time = core_reset_time,
            "Low GitHub API rate limit remaining"
        );
    }

    if rate_limit.resources.graphql.remaining < 10 {
        warn!(
            target: "autoprat::rate_limit",
            api_type = "graphql",
            remaining = rate_limit.resources.graphql.remaining,
            reset_time = graphql_reset_time,
            "Low GitHub GraphQL rate limit remaining"
        );
    }

    if rate_limit.resources.search.remaining < 5 {
        warn!(
            target: "autoprat::rate_limit",
            api_type = "search",
            remaining = rate_limit.resources.search.remaining,
            reset_time = search_reset_time,
            "Low GitHub search rate limit remaining"
        );
    }

    Ok(rate_limit.resources.core)
}

/// Helper function to execute GraphQL queries with enhanced error reporting
#[instrument(skip(octocrab, query), fields(query_type = "search_prs"))]
async fn execute_graphql_query(
    octocrab: &Octocrab,
    query: serde_json::Value,
    context: &str,
) -> Result<GraphQLResponse> {
    debug!("Executing GraphQL query");

    octocrab.graphql(&query).await.map_err(|e| {
        // Try to extract more specific error information.
        let error_msg = match &e {
            octocrab::Error::GitHub { source, .. } => {
                format!("GitHub API error: {source}")
            }
            octocrab::Error::Serde { source, .. } => {
                format!("JSON parsing error (likely rate limiting): {source}")
            }
            octocrab::Error::Http { source, .. } => {
                format!("HTTP error: {source}")
            }
            _ => format!("Unknown error: {e}"),
        };

        error!(
            context = context,
            error = %error_msg,
            "GraphQL query execution failed"
        );

        // For JSON parsing errors, suggest checking rate limits.
        if matches!(&e, octocrab::Error::Serde { .. }) {
            warn!("JSON parsing errors often indicate GitHub API rate limiting - check rate limit status above");
        }

        anyhow::anyhow!("{context}: {error_msg}")
    })
}

/// Simple GraphQL query builder that eliminates brittle JSON manipulation
/// while maintaining clean type boundaries - only github.rs knows about GraphQL
struct GraphQLQueryBuilder {
    query: String,
    variables: HashMap<String, serde_json::Value>,
}

impl GraphQLQueryBuilder {
    /// Create a new query builder for searching pull requests
    fn search_pull_requests() -> Self {
        Self {
            query: include_str!("github/search_prs.graphql").to_string(),
            variables: HashMap::new(),
        }
    }

    /// Set the search query string
    fn with_search_query(mut self, query: &str) -> Self {
        self.variables.insert("query".to_string(), query.into());
        self
    }

    fn with_after_cursor(mut self, cursor: Option<String>) -> Self {
        self.variables.insert(
            "after".to_string(),
            cursor.map_or(serde_json::Value::Null, |c| c.into()),
        );
        self
    }

    fn build(self) -> serde_json::Value {
        serde_json::json!({
            "query": self.query,
            "variables": self.variables
        })
    }
}

fn convert_conclusion(conclusion: Conclusion) -> CheckConclusion {
    match conclusion {
        Conclusion::Success => CheckConclusion::Success,
        Conclusion::Failure => CheckConclusion::Failure,
        Conclusion::Cancelled => CheckConclusion::Cancelled,
        Conclusion::TimedOut => CheckConclusion::TimedOut,
        Conclusion::ActionRequired => CheckConclusion::ActionRequired,
        Conclusion::Neutral => CheckConclusion::Neutral,
        Conclusion::Skipped => CheckConclusion::Skipped,
        _ => CheckConclusion::Neutral,
    }
}

fn convert_status_state(state: StatusState) -> CheckState {
    match state {
        StatusState::Success => CheckState::Success,
        StatusState::Failure => CheckState::Failure,
        StatusState::Pending => CheckState::Pending,
        StatusState::Error => CheckState::Error,
        _ => CheckState::Pending,
    }
}

fn convert_check_run_status(status: GraphQLCheckRunStatus) -> CheckRunStatus {
    match status {
        GraphQLCheckRunStatus::Queued => CheckRunStatus::Queued,
        GraphQLCheckRunStatus::InProgress => CheckRunStatus::InProgress,
        GraphQLCheckRunStatus::Completed => CheckRunStatus::Completed,
        GraphQLCheckRunStatus::Waiting => CheckRunStatus::Waiting,
        GraphQLCheckRunStatus::Requested => CheckRunStatus::Requested,
        GraphQLCheckRunStatus::Pending => CheckRunStatus::Pending,
    }
}

/// Custom deserialiser for GraphQL conclusion values.
///
/// Converts uppercase GraphQL enum values (e.g., "SUCCESS") to
/// Octocrab's Conclusion enum. Returns an error for unknown values.
fn deserialize_graphql_conclusion<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<Conclusion>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;

    let s = Option::<String>::deserialize(deserializer)?;
    let conclusion = s
        .as_ref()
        .map(|s| match s.as_str() {
            "SUCCESS" => Ok(Conclusion::Success),
            "FAILURE" => Ok(Conclusion::Failure),
            "CANCELLED" => Ok(Conclusion::Cancelled),
            "TIMED_OUT" => Ok(Conclusion::TimedOut),
            "ACTION_REQUIRED" => Ok(Conclusion::ActionRequired),
            "NEUTRAL" => Ok(Conclusion::Neutral),
            "SKIPPED" => Ok(Conclusion::Skipped),
            unknown => Err(Error::custom(format!(
                "Unknown GraphQL conclusion value: '{unknown}'"
            ))),
        })
        .transpose()?;
    Ok(conclusion)
}

/// Custom deserialiser for GraphQL status state values.
///
/// Converts GraphQL status values to Octocrab's StatusState enum.
/// Performs case-insensitive matching and returns errors for unknowns.
fn deserialize_graphql_status_state<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<StatusState>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;

    let s = Option::<String>::deserialize(deserializer)?;
    let state = s
        .as_ref()
        .map(|s| match s.to_lowercase().as_str() {
            "success" => Ok(StatusState::Success),
            "failure" => Ok(StatusState::Failure),
            "pending" => Ok(StatusState::Pending),
            "error" => Ok(StatusState::Error),
            unknown => Err(Error::custom(format!(
                "Unknown GraphQL status state value: '{unknown}'"
            ))),
        })
        .transpose()?;
    Ok(state)
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
enum ActorType {
    User,
    Bot,
    App,
    Organization,
    #[serde(other)]
    Unknown,
}

impl ActorType {
    fn is_bot(&self) -> bool {
        matches!(self, ActorType::Bot | ActorType::App)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum GraphQLCheckRunStatus {
    Queued,
    InProgress,
    Completed,
    Waiting,
    Requested,
    Pending,
}

fn deserialize_graphql_check_run_status<'de, D>(
    deserializer: D,
) -> Result<Option<GraphQLCheckRunStatus>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;

    let status: Option<String> = Option::deserialize(deserializer)?;
    let result = status
        .map(|s| match s.as_str() {
            "QUEUED" => Ok(GraphQLCheckRunStatus::Queued),
            "IN_PROGRESS" => Ok(GraphQLCheckRunStatus::InProgress),
            "COMPLETED" => Ok(GraphQLCheckRunStatus::Completed),
            "WAITING" => Ok(GraphQLCheckRunStatus::Waiting),
            "REQUESTED" => Ok(GraphQLCheckRunStatus::Requested),
            "PENDING" => Ok(GraphQLCheckRunStatus::Pending),
            unknown => Err(Error::custom(format!(
                "Unknown GraphQL check run status value: '{unknown}'"
            ))),
        })
        .transpose()?;
    Ok(result)
}

#[derive(Debug, Deserialize)]
#[serde(tag = "__typename")]
enum GraphQLStatusContext {
    CheckRun {
        name: Option<String>,
        #[serde(deserialize_with = "deserialize_graphql_check_run_status", default)]
        status: Option<GraphQLCheckRunStatus>,
        #[serde(deserialize_with = "deserialize_graphql_conclusion", default)]
        conclusion: Option<Conclusion>,
        #[serde(rename = "detailsUrl")]
        details_url: Option<String>,
    },
    StatusContext {
        context: Option<String>,
        #[serde(deserialize_with = "deserialize_graphql_status_state", default)]
        state: Option<StatusState>,
        #[serde(rename = "targetUrl")]
        target_url: Option<String>,
    },
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
    url: Url,
    created_at: DateTime<Utc>,
    base_ref_name: Option<String>,
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
    actor_type: ActorType,
}

impl GraphQLAuthor {
    fn search_format(&self) -> String {
        if self.actor_type.is_bot() {
            format!("app/{}", self.login)
        } else {
            self.login.clone()
        }
    }

    fn display_format(&self) -> String {
        if self.actor_type.is_bot() {
            format!("{}[bot]", self.login)
        } else {
            self.login.clone()
        }
    }

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
struct GraphQLCommentConnection {
    nodes: Vec<GraphQLComment>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GraphQLComment {
    body: String,
    created_at: DateTime<Utc>,
}

/// Obtains a GitHub authentication token from multiple sources.
///
/// Attempts to retrieve a token in the following order:
/// 1. GITHUB_TOKEN environment variable
/// 2. GH_TOKEN environment variable
/// 3. GitHub CLI (`gh auth token`)
///
/// Returns an error if no valid token can be obtained.
#[instrument]
async fn get_github_token() -> Result<String> {
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        debug!("Using GITHUB_TOKEN environment variable");
        return Ok(token);
    }

    if let Ok(token) = std::env::var("GH_TOKEN") {
        debug!("Using GH_TOKEN environment variable");
        return Ok(token);
    }

    debug!("Fetching token from gh CLI");
    let output = tokio::process::Command::new("gh")
        .args(["auth", "token"])
        .output()
        .await?;

    if !output.status.success() {
        error!("gh CLI authentication failed");
        anyhow::bail!("Failed to get GitHub token from gh CLI. Please run 'gh auth login' first");
    }

    let token = String::from_utf8(output.stdout)?.trim().to_string();

    if token.is_empty() {
        error!("Empty token returned from gh CLI");
        anyhow::bail!("Empty token returned from gh CLI");
    }

    debug!("Successfully obtained GitHub token");
    Ok(token)
}

/// Creates an authenticated GitHub API client.
///
/// Retrieves a GitHub token and initialises an Octocrab client
/// configured for API access.
#[instrument]
async fn setup_github_client() -> Result<Octocrab> {
    let token = get_github_token()
        .await
        .context("Failed to obtain GitHub authentication token")?;
    debug!("Creating GitHub client");
    Octocrab::builder()
        .personal_token(token)
        .build()
        .context("Failed to create GitHub client")
}

fn convert_graphql_status_context(context: GraphQLStatusContext) -> CheckInfo {
    match context {
        GraphQLStatusContext::CheckRun {
            name,
            status,
            conclusion,
            details_url,
        } => CheckInfo {
            name: CheckName::new(name.unwrap_or_else(|| "Unknown Check".to_string()))
                .unwrap_or_else(|_| CheckName::new("Unknown").unwrap()),
            conclusion: conclusion.map(convert_conclusion),
            run_status: status.map(convert_check_run_status),
            status_state: None,
            url: details_url.and_then(|url| CheckUrl::new(&url).ok()),
        },
        GraphQLStatusContext::StatusContext {
            context,
            state,
            target_url,
        } => CheckInfo {
            name: CheckName::new(context.unwrap_or_else(|| "Unknown Status".to_string()))
                .unwrap_or_else(|_| CheckName::new("Unknown").unwrap()),
            conclusion: None,
            run_status: None,
            status_state: state.map(convert_status_state),
            url: target_url.and_then(|url| CheckUrl::new(&url).ok()),
        },
    }
}

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

/// Converts a GraphQL pull request to our domain model.
///
/// Transforms GraphQL response data into a PullRequest struct,
/// including status checks, comments, and author information.
/// Requires an explicit Repo context.
fn convert_graphql_pr_to_pr_info(
    graphql_pr: GraphQLPullRequest,
    repo: Repo,
) -> Result<PullRequest> {
    let checks = convert_status_checks(graphql_pr.status_check_rollup);
    let recent_comments = convert_comments(graphql_pr.comments);

    Ok(PullRequest {
        repo,
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
        url: graphql_pr.url.to_string(),
        labels: graphql_pr
            .labels
            .nodes
            .into_iter()
            .map(|label| label.name)
            .collect(),
        created_at: graphql_pr.created_at,
        base_branch: graphql_pr
            .base_ref_name
            .ok_or_else(|| anyhow::anyhow!("PR {} missing base branch", graphql_pr.number))?,
        checks,
        recent_comments,
    })
}

/// Converts a GraphQL pull request to our domain model.
///
/// Fallback variant that extracts repository information from the PR's
/// URL when explicit repo context is unavailable.
fn convert_graphql_pr_to_pr_info_with_url_parsing(
    graphql_pr: GraphQLPullRequest,
) -> Result<PullRequest> {
    let (repo, _) = Repo::parse_url(graphql_pr.url.as_str())?;
    convert_graphql_pr_to_pr_info(graphql_pr, repo)
}

/// Fetches a single pull request using a search query.
///
/// Executes a GraphQL search expecting at most one result. Used for
/// fetching specific PRs by number when the repo context is known.
#[instrument(skip(octocrab), fields(query = %search_query, repo = %repo))]
async fn fetch_single_pr_by_query(
    octocrab: &Octocrab,
    search_query: &str,
    repo: Repo,
) -> Result<Option<PullRequest>> {
    debug!("Fetching single PR by query");
    let query = GraphQLQueryBuilder::search_pull_requests()
        .with_search_query(search_query)
        .with_after_cursor(None)
        .build();

    debug!(query_variables = ?query.get("variables"), "Executing single PR GraphQL query");

    let context = format!("Single PR query for repo {repo} with '{search_query}'");
    let response = execute_graphql_query(octocrab, query, &context).await?;

    if let Some(graphql_pr) = response.data.search.nodes.into_iter().next() {
        debug!(pr_number = graphql_pr.number, "Found PR");
        Ok(Some(convert_graphql_pr_to_pr_info(graphql_pr, repo)?))
    } else {
        debug!("No PR found for query");
        Ok(None)
    }
}

/// Collects multiple specific pull requests by their identifiers.
///
/// Fetches each PR individually using search queries. Validates that
/// returned PR numbers match the requested ones.
#[instrument(skip(octocrab), fields(pr_count = pr_identifiers.len()))]
async fn collect_specific_prs(
    octocrab: &Octocrab,
    pr_identifiers: &[(Repo, u64)],
) -> Result<Vec<PullRequest>> {
    info!("Collecting specific PRs");
    let mut all_prs = Vec::with_capacity(pr_identifiers.len());

    for (repo, number) in pr_identifiers {
        let search_query = format!("repo:{repo} type:pr {number}");

        if let Some(pr_info) =
            fetch_single_pr_by_query(octocrab, &search_query, repo.clone()).await?
        {
            if pr_info.number == *number {
                all_prs.push(pr_info);
            } else {
                warn!(
                    expected = number,
                    actual = pr_info.number,
                    "PR number mismatch"
                );
            }
        }
    }

    info!(found_count = all_prs.len(), "Collected specific PRs");
    Ok(all_prs)
}

/// Fetches pull requests using paginated GraphQL search.
///
/// Handles GitHub's pagination limits by making multiple requests.
/// Continues until the limit is reached or no more results exist.
/// Returns partial results on pagination errors rather than failing.
#[instrument(skip(octocrab), fields(query = %search_query, limit = limit, has_repo_context = repo.is_some()))]
async fn fetch_prs_with_pagination(
    octocrab: &Octocrab,
    search_query: &str,
    limit: usize,
    repo: Option<Repo>,
) -> Result<Vec<PullRequest>> {
    info!("Fetching PRs with pagination");
    let mut all_prs = Vec::with_capacity(limit.min(100)); // GitHub returns max 100 per page.
    let mut after_cursor: Option<String> = None;
    let mut processed_count = 0;
    let mut page_count = 0;

    loop {
        page_count += 1;
        debug!(page = page_count, cursor = ?after_cursor, "Fetching page");

        let query = GraphQLQueryBuilder::search_pull_requests()
            .with_search_query(search_query)
            .with_after_cursor(after_cursor.clone())
            .build();

        debug!(query_variables = ?query.get("variables"), "Executing GraphQL query");

        let context = format!("Pagination query page {page_count} for '{search_query}'");
        let response = match execute_graphql_query(octocrab, query, &context).await {
            Ok(response) => response,
            Err(e) => {
                warn!(
                    page = page_count,
                    cursor = ?after_cursor,
                    error = %e,
                    current_pr_count = all_prs.len(),
                    "GraphQL pagination failed, returning partial results"
                );
                // Return what we have so far rather than failing completely.
                break;
            }
        };

        let search_results = response.data.search;

        debug!(
            page_pr_count = search_results.nodes.len(),
            "Received PRs from GraphQL"
        );

        for graphql_pr in search_results.nodes {
            if processed_count >= limit {
                info!(
                    final_count = all_prs.len(),
                    pages = page_count,
                    "Reached limit"
                );
                return Ok(all_prs);
            }

            let pr_info = if let Some(ref repo) = repo {
                convert_graphql_pr_to_pr_info(graphql_pr, repo.clone())
            } else {
                convert_graphql_pr_to_pr_info_with_url_parsing(graphql_pr)
            };

            match pr_info {
                Ok(pr_info) => {
                    all_prs.push(pr_info);
                    processed_count += 1;
                }
                Err(e) => {
                    warn!(error = %e, "Failed to convert GraphQL PR");
                }
            }
        }

        if search_results.page_info.has_next_page {
            after_cursor = search_results.page_info.end_cursor;
        } else {
            info!(
                final_count = all_prs.len(),
                pages = page_count,
                "Completed pagination - no more pages"
            );
            break;
        }
    }

    info!(
        final_count = all_prs.len(),
        pages = page_count,
        requested_limit = limit,
        "Pagination completed"
    );
    Ok(all_prs)
}

/// Verifies that a repository exists on GitHub.
///
/// Makes a REST API call to check if the repository is accessible.
/// Returns an error if the repository doesn't exist or isn't accessible.
#[instrument(skip(octocrab), fields(repo = %repo))]
async fn verify_repository_exists(octocrab: &Octocrab, repo: &Repo) -> Result<()> {
    debug!("Verifying repository exists");

    let result = octocrab.repos(repo.owner(), repo.name()).get().await;

    match result {
        Ok(_) => {
            debug!("Repository verified");
            Ok(())
        }
        Err(octocrab::Error::GitHub { source, .. }) => {
            if source.message.contains("Not Found") {
                anyhow::bail!("Repository '{}' does not exist or is not accessible", repo)
            } else {
                anyhow::bail!("Failed to verify repository '{}': {}", repo, source.message)
            }
        }
        Err(e) => {
            anyhow::bail!("Failed to verify repository '{}': {}", repo, e)
        }
    }
}

/// Fetches pull request data from GitHub according to the query spec.
///
/// Handles both specific PR queries and search-based queries. Monitors
/// rate limits and provides detailed instrumentation of the operation.
#[instrument(skip(spec), fields(
    has_specific_prs = !spec.prs.is_empty(),
    pr_count = spec.prs.len(),
    repo_count = spec.repos.len(),
    has_custom_query = spec.query.is_some(),
    limit = spec.limit
))]
async fn fetch_github_data(spec: &crate::types::QuerySpec) -> Result<Vec<PullRequest>> {
    info!("Starting GitHub data fetch");
    let octocrab = setup_github_client().await?;

    // Check rate limit before starting (in debug mode).
    let rate_limit_before = check_rate_limit(&octocrab, "before GraphQL operations").await;
    if let Err(e) = &rate_limit_before {
        debug!("Rate limit check failed, continuing anyway: {}", e);
    }

    let result = if !spec.prs.is_empty() {
        debug!("Fetching specific PRs");
        collect_specific_prs(&octocrab, &spec.prs).await
    } else if spec.query.is_some() {
        debug!("Using custom query");
        let search_query = spec.query.as_ref().unwrap();
        fetch_prs_with_pagination(&octocrab, search_query, spec.limit, None).await
    } else if !spec.repos.is_empty() {
        debug!("Fetching PRs from {} repo(s)", spec.repos.len());

        // Verify all repositories exist before attempting to fetch PRs
        for repo in &spec.repos {
            verify_repository_exists(&octocrab, repo).await?;
        }

        let mut all_prs = Vec::new();
        for repo in &spec.repos {
            let search_query = repo.build_search_query(&spec.search_filters);
            let prs =
                fetch_prs_with_pagination(&octocrab, &search_query, spec.limit, Some(repo.clone()))
                    .await?;
            all_prs.extend(prs);
        }
        Ok(all_prs)
    } else {
        error!("No query available for search");
        anyhow::bail!("Query is required when not fetching specific PRs")
    };

    // Check rate limit after operations complete.
    let rate_limit_after = check_rate_limit(&octocrab, "after GraphQL operations").await;
    if let (Ok(before), Ok(after)) = (&rate_limit_before, &rate_limit_after) {
        let used_during_operation = before.remaining.saturating_sub(after.remaining);
        if used_during_operation > 0 {
            info!(
                rate_limit_used = used_during_operation,
                remaining_before = before.remaining,
                remaining_after = after.remaining,
                "GitHub API rate limit usage during operation"
            );
        }
    }

    result
}

/// GitHub forge implementation for fetching pull requests.
///
/// Provides access to GitHub's GraphQL API for querying pull requests,
/// their status checks, labels, and comments. Handles authentication
/// via environment variables or the GitHub CLI.
pub struct GitHub;

#[async_trait]
impl crate::types::Forge for GitHub {
    async fn fetch_pull_requests(
        &self,
        spec: &crate::types::QuerySpec,
    ) -> Result<Vec<PullRequest>> {
        fetch_github_data(spec).await
    }
}

#[cfg(test)]
mod tests {
    use chrono::DateTime;

    use super::*;

    #[test]
    fn test_graphql_query_builder() {
        let query = GraphQLQueryBuilder::search_pull_requests()
            .with_search_query("repo:owner/repo is:open")
            .with_after_cursor(None)
            .build();

        assert!(query.get("query").is_some());
        assert!(query.get("variables").is_some());

        let variables = query.get("variables").unwrap().as_object().unwrap();
        assert_eq!(
            variables.get("query").unwrap().as_str().unwrap(),
            "repo:owner/repo is:open"
        );
        assert!(variables.get("after").unwrap().is_null());
    }

    #[test]
    fn test_graphql_query_builder_with_cursor() {
        let query = GraphQLQueryBuilder::search_pull_requests()
            .with_search_query("repo:owner/repo")
            .with_after_cursor(Some("cursor123".to_string()))
            .build();

        let variables = query.get("variables").unwrap().as_object().unwrap();
        assert_eq!(
            variables.get("after").unwrap().as_str().unwrap(),
            "cursor123"
        );
    }

    #[test]
    fn test_query_includes_graphql_content() {
        let query = GraphQLQueryBuilder::search_pull_requests()
            .with_search_query("test")
            .with_after_cursor(None)
            .build();

        let query_str = query.get("query").unwrap().as_str().unwrap();

        // Verify key GraphQL elements are present.
        assert!(query_str.contains("query($query: String!, $after: String)"));
        assert!(query_str.contains("search(query: $query"));
        assert!(query_str.contains("... on PullRequest"));
        assert!(query_str.contains("pageInfo"));
    }

    fn create_test_graphql_pr() -> GraphQLPullRequest {
        GraphQLPullRequest {
            number: 123,
            title: "Test PR".to_string(),
            url: Url::parse("https://github.com/owner/repo/pull/123").unwrap(),
            created_at: DateTime::from_timestamp(1609459200, 0).unwrap(), // 2021-01-01.
            base_ref_name: Some("main".to_string()),
            author: Some(GraphQLAuthor {
                login: "testuser".to_string(),
                actor_type: ActorType::User,
            }),
            labels: GraphQLLabelConnection {
                nodes: vec![
                    GraphQLLabel {
                        name: "bug".to_string(),
                    },
                    GraphQLLabel {
                        name: "priority/high".to_string(),
                    },
                ],
            },
            status_check_rollup: Some(GraphQLStatusCheckRollup {
                contexts: GraphQLStatusContextConnection {
                    nodes: vec![
                        GraphQLStatusContext::CheckRun {
                            name: Some("test-check".to_string()),
                            status: Some(GraphQLCheckRunStatus::Completed),
                            conclusion: Some(Conclusion::Success),
                            details_url: Some("https://example.com/check/1".to_string()),
                        },
                        GraphQLStatusContext::StatusContext {
                            context: Some("ci/build".to_string()),
                            state: Some(StatusState::Failure),
                            target_url: Some("https://example.com/build/1".to_string()),
                        },
                    ],
                },
            }),
            comments: GraphQLCommentConnection {
                nodes: vec![
                    GraphQLComment {
                        body: "/lgtm".to_string(),
                        created_at: DateTime::from_timestamp(1609459300, 0).unwrap(),
                    },
                    GraphQLComment {
                        body: "Looks good to me!".to_string(),
                        created_at: DateTime::from_timestamp(1609459400, 0).unwrap(),
                    },
                ],
            },
        }
    }

    #[test]
    fn test_convert_graphql_pr_to_pr_info_with_repo_context() {
        let graphql_pr = create_test_graphql_pr();
        let repo = Repo::new("owner".to_string(), "repo".to_string()).unwrap();

        let result = convert_graphql_pr_to_pr_info(graphql_pr, repo.clone());
        assert!(result.is_ok());

        let pr_info = result.unwrap();
        assert_eq!(pr_info.repo, repo);
        assert_eq!(pr_info.number, 123);
        assert_eq!(pr_info.title, "Test PR");
        assert_eq!(pr_info.author_login, "testuser");
        assert_eq!(pr_info.author_search_format, "testuser");
        assert_eq!(pr_info.author_simple_name, "testuser");
        assert_eq!(pr_info.url, "https://github.com/owner/repo/pull/123");
        assert_eq!(pr_info.labels, vec!["bug", "priority/high"]);
        assert_eq!(pr_info.checks.len(), 2);
        assert_eq!(pr_info.recent_comments.len(), 2);

        let check1 = &pr_info.checks[0];
        assert_eq!(check1.name.as_str(), "test-check");
        assert_eq!(check1.conclusion, Some(CheckConclusion::Success));
        assert_eq!(check1.status_state, None);

        let check2 = &pr_info.checks[1];
        assert_eq!(check2.name.as_str(), "ci/build");
        assert_eq!(check2.conclusion, None);
        assert_eq!(check2.status_state, Some(CheckState::Failure));
    }

    #[test]
    fn test_convert_graphql_pr_to_pr_info_with_url_parsing() {
        let graphql_pr = create_test_graphql_pr();

        let result = convert_graphql_pr_to_pr_info_with_url_parsing(graphql_pr);
        assert!(result.is_ok());

        let pr_info = result.unwrap();
        assert_eq!(pr_info.repo.owner(), "owner");
        assert_eq!(pr_info.repo.name(), "repo");
        assert_eq!(pr_info.number, 123);
        assert_eq!(pr_info.title, "Test PR");
    }

    #[test]
    fn test_convert_graphql_pr_to_pr_info_with_bot_author() {
        let mut graphql_pr = create_test_graphql_pr();
        graphql_pr.author = Some(GraphQLAuthor {
            login: "dependabot".to_string(),
            actor_type: ActorType::Bot,
        });

        let repo = Repo::new("owner".to_string(), "repo".to_string()).unwrap();
        let result = convert_graphql_pr_to_pr_info(graphql_pr, repo);
        assert!(result.is_ok());

        let pr_info = result.unwrap();
        assert_eq!(pr_info.author_login, "dependabot[bot]");
        assert_eq!(pr_info.author_search_format, "app/dependabot");
        assert_eq!(pr_info.author_simple_name, "dependabot");
    }

    #[test]
    fn test_convert_graphql_pr_to_pr_info_with_no_author() {
        let mut graphql_pr = create_test_graphql_pr();
        graphql_pr.author = None;

        let repo = Repo::new("owner".to_string(), "repo".to_string()).unwrap();
        let result = convert_graphql_pr_to_pr_info(graphql_pr, repo);
        assert!(result.is_ok());

        let pr_info = result.unwrap();
        assert_eq!(pr_info.author_login, "Unknown");
        assert_eq!(pr_info.author_search_format, "Unknown");
        assert_eq!(pr_info.author_simple_name, "Unknown");
    }

    #[test]
    fn test_convert_graphql_pr_to_pr_info_with_no_checks() {
        let mut graphql_pr = create_test_graphql_pr();
        graphql_pr.status_check_rollup = None;

        let repo = Repo::new("owner".to_string(), "repo".to_string()).unwrap();
        let result = convert_graphql_pr_to_pr_info(graphql_pr, repo);
        assert!(result.is_ok());

        let pr_info = result.unwrap();
        assert_eq!(pr_info.checks.len(), 0);
    }

    #[test]
    fn test_convert_graphql_pr_to_pr_info_with_invalid_check_names() {
        let mut graphql_pr = create_test_graphql_pr();
        graphql_pr.status_check_rollup = Some(GraphQLStatusCheckRollup {
            contexts: GraphQLStatusContextConnection {
                nodes: vec![
                    GraphQLStatusContext::CheckRun {
                        name: None, // Missing name.
                        status: Some(GraphQLCheckRunStatus::Completed),
                        conclusion: Some(Conclusion::Success),
                        details_url: Some("https://example.com/check/1".to_string()),
                    },
                    GraphQLStatusContext::StatusContext {
                        context: Some("".to_string()), // Empty name.
                        state: Some(StatusState::Success),
                        target_url: None,
                    },
                ],
            },
        });

        let repo = Repo::new("owner".to_string(), "repo".to_string()).unwrap();
        let result = convert_graphql_pr_to_pr_info(graphql_pr, repo);
        assert!(result.is_ok());

        let pr_info = result.unwrap();
        assert_eq!(pr_info.checks.len(), 2);

        assert_eq!(pr_info.checks[0].name.as_str(), "Unknown Check");
        assert_eq!(pr_info.checks[1].name.as_str(), "Unknown");
    }

    #[test]
    fn test_convert_graphql_pr_to_pr_info_with_invalid_urls() {
        let mut graphql_pr = create_test_graphql_pr();
        graphql_pr.status_check_rollup = Some(GraphQLStatusCheckRollup {
            contexts: GraphQLStatusContextConnection {
                nodes: vec![GraphQLStatusContext::CheckRun {
                    name: Some("test-check".to_string()),
                    status: Some(GraphQLCheckRunStatus::Completed),
                    conclusion: Some(Conclusion::Success),
                    details_url: Some("not-a-valid-url".to_string()),
                }],
            },
        });

        let repo = Repo::new("owner".to_string(), "repo".to_string()).unwrap();
        let result = convert_graphql_pr_to_pr_info(graphql_pr, repo);
        assert!(result.is_ok());

        let pr_info = result.unwrap();
        assert_eq!(pr_info.checks.len(), 1);
        assert!(pr_info.checks[0].url.is_none());
    }

    #[test]
    fn test_deserialize_graphql_conclusion_valid() {
        #[derive(Debug, serde::Deserialize)]
        struct TestStruct {
            #[serde(deserialize_with = "deserialize_graphql_conclusion")]
            conclusion: Option<Conclusion>,
        }

        let result: TestStruct = serde_json::from_str(r#"{"conclusion": "SUCCESS"}"#).unwrap();
        assert_eq!(result.conclusion, Some(Conclusion::Success));

        let result: TestStruct = serde_json::from_str(r#"{"conclusion": "FAILURE"}"#).unwrap();
        assert_eq!(result.conclusion, Some(Conclusion::Failure));

        let result: TestStruct = serde_json::from_str(r#"{"conclusion": null}"#).unwrap();
        assert_eq!(result.conclusion, None);
    }

    #[test]
    fn test_deserialize_graphql_conclusion_invalid() {
        let invalid_value = serde_json::Value::String("INVALID_CONCLUSION".to_string());
        let deserializer_str = serde_json::to_string(&invalid_value).unwrap();
        let mut deserializer = serde_json::Deserializer::from_str(&deserializer_str);
        let result = deserialize_graphql_conclusion(&mut deserializer);
        assert!(result.is_err());

        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Unknown GraphQL conclusion value: 'INVALID_CONCLUSION'"));
    }

    #[test]
    fn test_deserialize_graphql_status_state_valid() {
        #[derive(Debug, serde::Deserialize)]
        struct TestStruct {
            #[serde(deserialize_with = "deserialize_graphql_status_state")]
            state: Option<StatusState>,
        }

        let result: TestStruct = serde_json::from_str(r#"{"state": "SUCCESS"}"#).unwrap();
        assert_eq!(result.state, Some(StatusState::Success));

        let result: TestStruct = serde_json::from_str(r#"{"state": "failure"}"#).unwrap(); // Test case insensitive.
        assert_eq!(result.state, Some(StatusState::Failure));

        let result: TestStruct = serde_json::from_str(r#"{"state": null}"#).unwrap();
        assert_eq!(result.state, None);
    }

    #[test]
    fn test_deserialize_graphql_status_state_invalid() {
        let invalid_value = serde_json::Value::String("INVALID_STATE".to_string());
        let deserializer_str = serde_json::to_string(&invalid_value).unwrap();
        let mut deserializer = serde_json::Deserializer::from_str(&deserializer_str);
        let result = deserialize_graphql_status_state(&mut deserializer);
        assert!(result.is_err());

        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Unknown GraphQL status state value: 'invalid_state'"));
    }

    #[test]
    fn test_actor_type_is_bot() {
        assert!(ActorType::Bot.is_bot());
        assert!(ActorType::App.is_bot());
        assert!(!ActorType::User.is_bot());
        assert!(!ActorType::Organization.is_bot());
        assert!(!ActorType::Unknown.is_bot());
    }

    #[test]
    fn test_graphql_author_formats() {
        let user_author = GraphQLAuthor {
            login: "testuser".to_string(),
            actor_type: ActorType::User,
        };
        assert_eq!(user_author.search_format(), "testuser");
        assert_eq!(user_author.display_format(), "testuser");
        assert_eq!(user_author.simple_name(), "testuser");

        let bot_author = GraphQLAuthor {
            login: "dependabot".to_string(),
            actor_type: ActorType::Bot,
        };
        assert_eq!(bot_author.search_format(), "app/dependabot");
        assert_eq!(bot_author.display_format(), "dependabot[bot]");
        assert_eq!(bot_author.simple_name(), "dependabot");
    }

    #[test]
    fn test_convert_conclusion() {
        assert_eq!(
            convert_conclusion(Conclusion::Success),
            CheckConclusion::Success
        );
        assert_eq!(
            convert_conclusion(Conclusion::Failure),
            CheckConclusion::Failure
        );
        assert_eq!(
            convert_conclusion(Conclusion::Cancelled),
            CheckConclusion::Cancelled
        );
        assert_eq!(
            convert_conclusion(Conclusion::TimedOut),
            CheckConclusion::TimedOut
        );
        assert_eq!(
            convert_conclusion(Conclusion::ActionRequired),
            CheckConclusion::ActionRequired
        );
        assert_eq!(
            convert_conclusion(Conclusion::Neutral),
            CheckConclusion::Neutral
        );
        assert_eq!(
            convert_conclusion(Conclusion::Skipped),
            CheckConclusion::Skipped
        );
    }

    #[test]
    fn test_convert_status_state() {
        assert_eq!(
            convert_status_state(StatusState::Success),
            CheckState::Success
        );
        assert_eq!(
            convert_status_state(StatusState::Failure),
            CheckState::Failure
        );
        assert_eq!(
            convert_status_state(StatusState::Pending),
            CheckState::Pending
        );
        assert_eq!(convert_status_state(StatusState::Error), CheckState::Error);
    }
}
