use anyhow::{Context, Result};
use octocrab::{
    Octocrab,
    models::{StatusState, workflows::Conclusion},
};

use crate::{
    github::{parse_pr_url, parse_repo_from_string},
    graphql::{convert_graphql_pr_to_pr_info, create_graphql_query, fetch_single_pr_by_query},
    types::*,
};

#[derive(Debug)]
pub struct SearchQueryBuilder {
    terms: Vec<String>,
}

impl SearchQueryBuilder {
    pub fn new() -> Self {
        Self { terms: Vec::new() }
    }

    pub fn repo(&mut self, owner: &str, name: &str) -> &mut Self {
        self.terms.push(format!("repo:{}/{}", owner, name));
        self
    }

    pub fn pr_type(&mut self) -> &mut Self {
        self.terms.push("type:pr".to_string());
        self
    }

    pub fn state(&mut self, state: SearchState) -> &mut Self {
        self.terms.push(format!("state:{}", state.as_str()));
        self
    }

    pub fn label(&mut self, label: &str) -> &mut Self {
        self.terms.push(format!("label:{}", label));
        self
    }

    pub fn no_label(&mut self, label: &str) -> &mut Self {
        self.terms.push(format!("-label:{}", label));
        self
    }

    pub fn status(&mut self, status: SearchStatus) -> &mut Self {
        self.terms.push(format!("status:{}", status.as_str()));
        self
    }

    pub fn build(&self) -> String {
        self.terms.join(" ")
    }
}

pub fn has_label(pr: &SimplePR, known_label: KnownLabel) -> bool {
    pr.labels.iter().any(|label| label == known_label.as_str())
}

pub fn has_specific_failing_check(checks: &[CheckInfo], check_name: &str) -> bool {
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
pub fn matches_author_filter(pr: &SimplePR, author: &str) -> bool {
    pr.author_login == author
        || pr.author_search_format == author
        || (pr.author_login.starts_with(&format!("{}[", author)) && pr.author_login.ends_with("]"))
        || (pr.author_search_format == format!("app/{}", author))
}

/// Tests if a PR matches label-based filtering requirements.
pub fn matches_label_filter(pr: &SimplePR, cli: &crate::Cli) -> bool {
    !cli.needs_ok_to_test || has_label(pr, KnownLabel::NeedsOkToTest)
}

/// Tests if a PR has all specified failing checks.
pub fn matches_failing_check_filters(pr_info: &PrInfo, failing_checks: &[String]) -> bool {
    failing_checks
        .iter()
        .all(|check_name| has_specific_failing_check(&pr_info.checks, check_name))
}

pub fn apply_remaining_filters(prs: Vec<PrInfo>, cli: &crate::Cli) -> Vec<PrInfo> {
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

/// Constructs a GitHub search query string from CLI filters and repository
/// information.
pub fn build_search_query_from_cli(repo: &str, cli: &crate::Cli) -> Result<String> {
    // If raw query is provided, ensure it includes is:pr and is:open
    if let Some(query) = &cli.query {
        let mut final_query = query.clone();

        // Add is:pr if not present
        if !final_query.contains("is:pr") {
            final_query = format!("{} is:pr", final_query);
        }

        // Add is:open if no state specified
        if !final_query.contains("is:open") && !final_query.contains("is:closed") {
            final_query = format!("{} is:open", final_query);
        }

        return Ok(final_query);
    }

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
        if let Some(label_name) = label.strip_prefix('-') {
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

// Streaming processor that accumulates results from all pages before displaying
pub async fn process_prs_streaming(
    octocrab: &Octocrab,
    search_query: &str,
    cli: &crate::Cli,
) -> Result<(Vec<String>, Vec<PrInfo>)> {
    let mut action_commands = Vec::new();
    let mut all_prs = Vec::new();
    let has_actions = crate::commands::has_action_commands(cli);
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
                let commands =
                    crate::commands::generate_action_commands(&all_prs[all_prs.len() - 1..], cli)?;
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

pub async fn collect_specific_prs(octocrab: &Octocrab, cli: &crate::Cli) -> Result<Vec<PrInfo>> {
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
