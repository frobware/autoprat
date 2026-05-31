//! GitHub forge adapter: the IO that talks to GitHub and produces the
//! forge-neutral domain model.
//!
//! This module owns authentication, the Octocrab client, rate-limit
//! checks, GraphQL execution, pagination, and the [`Forge`] impl. The
//! pure pieces it drives live in submodules: [`graphql`] (the wire
//! protocol and query builder) and [`convert`] (wire -> domain). All
//! three together are the GitHub adapter; only the domain model they
//! produce is shared with other forges.

mod convert;
mod graphql;
mod render;
mod search;

use anyhow::{Context, Result};
use async_trait::async_trait;
use convert::{convert_graphql_pr_to_pr_info, convert_graphql_pr_to_pr_info_with_url_parsing};
use graphql::{GraphQLQueryBuilder, GraphQLResponse};
use octocrab::Octocrab;
pub use render::GhCliRenderer;
use serde::Deserialize;
use tracing::{debug, error, info, instrument, warn};

use crate::{
    pr_selector::PrIdentifier,
    search::FetchPlan,
    types::{PullRequest, Repo},
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
    pr_identifiers: &[PrIdentifier],
) -> Result<Vec<PullRequest>> {
    info!("Collecting specific PRs");
    let mut all_prs = Vec::with_capacity(pr_identifiers.len());

    for identifier in pr_identifiers {
        let repo = &identifier.repo;
        let number = identifier.number;
        let search_query = search::build_specific_pr_search_query(repo, number);

        if let Some(pr_info) =
            fetch_single_pr_by_query(octocrab, &search_query, repo.clone()).await?
        {
            if is_requested_pr_number(&pr_info, number) {
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

fn is_requested_pr_number(pr: &PullRequest, requested: u64) -> bool {
    pr.number == requested
}

fn take_until_limit(
    page: impl IntoIterator<Item = PullRequest>,
    already: usize,
    limit: usize,
) -> (Vec<PullRequest>, bool) {
    let remaining = limit.saturating_sub(already);
    if remaining == 0 {
        return (Vec::new(), true);
    }

    let taken = page.into_iter().take(remaining).collect::<Vec<_>>();
    let reached_limit = taken.len() == remaining;

    (taken, reached_limit)
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

        let page_prs = search_results
            .nodes
            .into_iter()
            .filter_map(|graphql_pr| {
                let pr_info = if let Some(ref repo) = repo {
                    convert_graphql_pr_to_pr_info(graphql_pr, repo.clone())
                } else {
                    convert_graphql_pr_to_pr_info_with_url_parsing(graphql_pr)
                };

                match pr_info {
                    Ok(pr_info) => Some(pr_info),
                    Err(e) => {
                        warn!(error = %e, "Failed to convert GraphQL PR");
                        None
                    }
                }
            })
            .collect::<Vec<_>>();

        let (page_prs, reached_limit) = take_until_limit(page_prs, all_prs.len(), limit);
        all_prs.extend(page_prs);

        if reached_limit {
            info!(
                final_count = all_prs.len(),
                pages = page_count,
                "Reached limit"
            );
            return Ok(all_prs);
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

/// Fetches pull request data from GitHub according to a fetch plan.
///
/// Handles both specific PR queries and search-based queries. Monitors
/// rate limits and provides detailed instrumentation of the operation.
#[instrument(skip(plan), fields(plan = ?plan))]
async fn fetch_github_data(plan: &FetchPlan) -> Result<Vec<PullRequest>> {
    info!("Starting GitHub data fetch");
    let octocrab = setup_github_client().await?;

    // Check rate limit before starting (in debug mode).
    let rate_limit_before = check_rate_limit(&octocrab, "before GraphQL operations").await;
    if let Err(e) = &rate_limit_before {
        debug!("Rate limit check failed, continuing anyway: {}", e);
    }

    let result = match plan {
        FetchPlan::SpecificPullRequests(identifiers) => {
            debug!("Fetching specific PRs");
            collect_specific_prs(&octocrab, identifiers).await
        }
        FetchPlan::UserSearch { query, limit } => {
            debug!("Using custom query");
            let search_query = search::format_user_query(query);
            fetch_prs_with_pagination(&octocrab, &search_query, *limit, None).await
        }
        FetchPlan::RepositorySearches(searches) => {
            debug!("Fetching PRs from {} repo(s)", searches.len());

            for search in searches {
                verify_repository_exists(&octocrab, &search.repo).await?;
            }

            let mut all_prs = Vec::new();
            for search in searches {
                let search_query = search::build_repo_search_query(&search.repo, &search.criteria);
                let prs = fetch_prs_with_pagination(
                    &octocrab,
                    &search_query,
                    search.limit,
                    Some(search.repo.clone()),
                )
                .await?;
                all_prs.extend(prs);
            }
            Ok(all_prs)
        }
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
    async fn fetch_pull_requests(&self, plan: &FetchPlan) -> Result<Vec<PullRequest>> {
        fetch_github_data(plan).await
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;

    fn pr(number: u64) -> PullRequest {
        PullRequest {
            repo: Repo::new("owner", "repo").unwrap(),
            number,
            title: format!("PR {number}"),
            author_login: "alice".to_string(),
            author_simple_name: "alice".to_string(),
            url: format!("https://github.com/owner/repo/pull/{number}"),
            labels: vec![],
            created_at: Utc.with_ymd_and_hms(2026, 5, 29, 12, 0, 0).unwrap(),
            base_branch: "main".to_string(),
            commit_count: 1,
            checks: vec![],
            recent_comments: vec![],
        }
    }

    fn pr_numbers(prs: &[PullRequest]) -> Vec<u64> {
        prs.iter().map(|pr| pr.number).collect()
    }

    #[test]
    fn requested_pr_number_keeps_only_exact_matches() {
        assert!(is_requested_pr_number(&pr(123), 123));
        assert!(!is_requested_pr_number(&pr(124), 123));
    }

    #[test]
    fn take_until_limit_keeps_only_remaining_budget() {
        let (taken, reached_limit) =
            take_until_limit(vec![pr(1), pr(2), pr(3), pr(4), pr(5)], 8, 10);

        assert_eq!(pr_numbers(&taken), vec![1, 2]);
        assert!(reached_limit);
    }

    #[test]
    fn take_until_limit_reports_exact_limit() {
        let (taken, reached_limit) = take_until_limit(vec![pr(1), pr(2)], 8, 10);

        assert_eq!(pr_numbers(&taken), vec![1, 2]);
        assert!(reached_limit);
    }

    #[test]
    fn take_until_limit_reports_under_limit() {
        let (taken, reached_limit) = take_until_limit(vec![pr(1)], 8, 10);

        assert_eq!(pr_numbers(&taken), vec![1]);
        assert!(!reached_limit);
    }

    #[test]
    fn take_until_limit_handles_empty_pages() {
        let (taken, reached_limit) = take_until_limit(Vec::new(), 8, 10);

        assert!(taken.is_empty());
        assert!(!reached_limit);
    }

    #[test]
    fn take_until_limit_stops_when_already_at_limit() {
        let (taken, reached_limit) = take_until_limit(vec![pr(1)], 10, 10);

        assert!(taken.is_empty());
        assert!(reached_limit);
    }
}
