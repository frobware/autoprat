use std::time::Duration;

use crate::{
    cli::CommentAction,
    types::{Action, Forge, PullRequest, QueryResult, QuerySpec, Task},
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
    let all_prs = forge.fetch_pull_requests(request).await?;

    let filtered_prs: Vec<PullRequest> = all_prs
        .into_iter()
        .filter(|pr| pr.matches_request(request))
        .collect();

    let executable_actions = generate_executable_actions(
        &filtered_prs,
        &request.actions,
        &request.custom_comments,
        &request.throttle,
    );

    Ok(QueryResult {
        filtered_prs,
        executable_actions,
    })
}

fn generate_executable_actions(
    filtered_prs: &[PullRequest],
    actions: &[Box<dyn Action + Send + Sync>],
    custom_comments: &[String],
    throttle: &Option<Duration>,
) -> Vec<Task> {
    let mut executable_actions =
        Vec::with_capacity(filtered_prs.len() * (actions.len() + custom_comments.len()));

    for pr in filtered_prs {
        for action in actions {
            if action.only_if(pr) {
                if let Some(body) = action.get_comment_body() {
                    // When there's no throttling, we're permissive
                    // and allow all actions. When throttling is
                    // enabled, we're restrictive and only allow
                    // actions we haven't done recently.
                    if throttle
                        .is_none_or(|duration| !pr.was_comment_posted_recently(body, duration))
                    {
                        executable_actions.push(Task {
                            pr_info: pr.clone(),
                            action: action.clone(),
                        });
                    }
                } else {
                    executable_actions.push(Task {
                        pr_info: pr.clone(),
                        action: action.clone(),
                    });
                }
            }
        }

        for comment in custom_comments {
            if throttle.is_none_or(|duration| !pr.was_comment_posted_recently(comment, duration)) {
                let custom_action = CommentAction::new(comment.clone());
                if custom_action.only_if(pr) {
                    executable_actions.push(Task {
                        pr_info: pr.clone(),
                        action: Box::new(custom_action),
                    });
                }
            }
        }
    }

    executable_actions
}
