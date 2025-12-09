use crate::types::{Forge, PullRequest, QueryResult, QuerySpec, Task};

/// Fetches and filters pull requests according to the query specification.
///
/// Retrieves PRs from the forge, applies post-filters, and generates
/// executable actions based on the query's action list and throttling
/// settings. Returns both filtered PRs and actions ready for execution.
pub async fn fetch_pull_requests<F>(request: &QuerySpec, forge: &F) -> anyhow::Result<QueryResult>
where
    F: Forge + Sync,
{
    let all_prs = forge.fetch_pull_requests(request).await?;

    let filtered_prs: Vec<PullRequest> = all_prs
        .into_iter()
        .filter(|pr| pr.matches_request(request))
        .collect();

    let executable_actions = generate_executable_actions(&filtered_prs, request);

    Ok(QueryResult {
        filtered_prs,
        executable_actions,
    })
}

fn generate_executable_actions(filtered_prs: &[PullRequest], request: &QuerySpec) -> Vec<Task> {
    let mut executable_actions = Vec::with_capacity(filtered_prs.len() * request.actions.len());

    for pr in filtered_prs {
        for action in &request.actions {
            if action.should_execute(
                pr,
                request.history_max_comments,
                request.history_max_age,
                request.throttle,
            ) {
                executable_actions.push(Task {
                    pr_info: pr.clone(),
                    action: action.clone(),
                });
            }
        }
    }

    executable_actions
}
