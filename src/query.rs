use chrono::Utc;

use crate::{
    decision::{commit_limit_offenders, generate_executable_actions, pull_request_matches},
    search::FetchPlan,
    types::{Forge, PullRequest, QueryResult, QuerySpec, Task},
};

/// Fetches and filters pull requests according to the query specification.
///
/// Retrieves PRs from the forge, applies post-filters, and generates
/// executable actions based on the query's action list and throttling
/// settings. Returns both filtered PRs and actions ready for execution.
pub async fn fetch_pull_requests<F>(request: &QuerySpec, forge: &F) -> anyhow::Result<QueryResult>
where
    F: Forge + Sync,
{
    let fetch_plan = FetchPlan::from_criteria(&request.fetch)
        .ok_or_else(|| anyhow::anyhow!("Query is required when not fetching specific PRs"))?;
    let all_prs = forge.fetch_pull_requests(&fetch_plan).await?;

    let filtered_prs: Vec<PullRequest> = all_prs
        .into_iter()
        .filter(|pr| pull_request_matches(pr, &request.fetch, &request.selection))
        .collect();

    let executable_actions =
        generate_executable_actions(&filtered_prs, &request.action_policy, Utc::now());

    enforce_commit_limit(&executable_actions, request.action_policy.commit_limit)?;

    Ok(QueryResult {
        filtered_prs,
        executable_actions,
    })
}

fn enforce_commit_limit(tasks: &[Task], limit: u64) -> anyhow::Result<()> {
    let offenders = commit_limit_offenders(tasks, limit);

    if offenders.is_empty() {
        return Ok(());
    }

    let mut msg = format!(
        "{} pull request(s) exceed --commit-limit={} (no commands emitted):",
        offenders.len(),
        limit
    );
    for offender in &offenders {
        msg.push_str(&format!(
            "\n  {} ({} commits)",
            offender.url, offender.commit_count
        ));
    }
    msg.push_str(
        "\nRe-run with --commit-limit <N> to raise the threshold, or --exclude the PR(s).",
    );
    anyhow::bail!(msg)
}
