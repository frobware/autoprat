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

    let executable_actions = generate_executable_actions(&filtered_prs, request);

    Ok(QueryResult {
        filtered_prs,
        executable_actions,
    })
}

fn generate_executable_actions(filtered_prs: &[PullRequest], request: &QuerySpec) -> Vec<Task> {
    let mut executable_actions = Vec::with_capacity(
        filtered_prs.len() * (request.actions.len() + request.custom_comments.len()),
    );

    for pr in filtered_prs {
        for action in &request.actions {
            if action.only_if(pr) {
                if let Some(body) = action.get_comment_body() {
                    // For idempotent actions, check if we've already posted
                    // this comment in the recent history. This prevents
                    // re-posting commands when GitHub is slow to apply labels.
                    if pr.was_comment_posted_in_history(
                        body,
                        request.history_max_comments,
                        request.history_max_age,
                    ) {
                        continue;
                    }

                    // Additionally apply throttle check if enabled.
                    if request
                        .throttle
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

        for comment in &request.custom_comments {
            // For custom comments, also check history first.
            if pr.was_comment_posted_in_history(
                comment,
                request.history_max_comments,
                request.history_max_age,
            ) {
                continue;
            }

            if request
                .throttle
                .is_none_or(|duration| !pr.was_comment_posted_recently(comment, duration))
            {
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
