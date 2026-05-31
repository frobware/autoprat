//! Conversion from GitHub's GraphQL wire types ([`super::graphql`]) to
//! the forge-neutral domain model ([`crate::types`]).
//!
//! This is the GitHub adapter's anti-corruption layer: the single place
//! GitHub's response shape touches the shared domain. It is IO-free and
//! maps shape only -- filtering, ranking, bot-means-ignore, and task
//! decisions stay outside the adapter, in the forge-neutral core. A
//! GitLab adapter would provide its own equivalent, converging on the
//! same domain types; nothing here is shared, but the model it produces
//! is.

use anyhow::Result;
use octocrab::models::{StatusState, workflows::Conclusion};

use super::graphql::{
    GraphQLCheckRunStatus, GraphQLCommentConnection, GraphQLPullRequest, GraphQLStatusCheckRollup,
    GraphQLStatusContext,
};
use crate::types::{
    CheckConclusion, CheckInfo, CheckName, CheckRunStatus, CheckState, CheckUrl, CommentInfo,
    PullRequest, Repo,
};

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
pub(crate) fn convert_graphql_pr_to_pr_info(
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
        commit_count: graphql_pr.commits.total_count,
        checks,
        recent_comments,
    })
}

/// Converts a GraphQL pull request to our domain model.
///
/// Fallback variant that extracts repository information from the PR's
/// URL when explicit repo context is unavailable.
pub(crate) fn convert_graphql_pr_to_pr_info_with_url_parsing(
    graphql_pr: GraphQLPullRequest,
) -> Result<PullRequest> {
    let repo = Repo::parse_url(graphql_pr.url.as_str())?;
    convert_graphql_pr_to_pr_info(graphql_pr, repo)
}

#[cfg(test)]
mod tests {
    use chrono::DateTime;
    use url::Url;

    use super::{super::graphql::*, *};

    fn create_test_graphql_pr() -> GraphQLPullRequest {
        GraphQLPullRequest {
            number: 123,
            title: "Test PR".to_string(),
            url: Url::parse("https://github.com/owner/repo/pull/123").unwrap(),
            created_at: DateTime::from_timestamp(1609459200, 0).unwrap(), // 2021-01-01.
            base_ref_name: Some("main".to_string()),
            commits: GraphQLCommitConnection { total_count: 1 },
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
        let repo = Repo::new("owner", "repo").unwrap();

        let result = convert_graphql_pr_to_pr_info(graphql_pr, repo.clone());
        assert!(result.is_ok());

        let pr_info = result.unwrap();
        assert_eq!(pr_info.repo, repo);
        assert_eq!(pr_info.number, 123);
        assert_eq!(pr_info.title, "Test PR");
        assert_eq!(pr_info.author_login, "testuser");
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

        let repo = Repo::new("owner", "repo").unwrap();
        let result = convert_graphql_pr_to_pr_info(graphql_pr, repo);
        assert!(result.is_ok());

        let pr_info = result.unwrap();
        assert_eq!(pr_info.author_login, "dependabot[bot]");
        assert_eq!(pr_info.author_simple_name, "dependabot");
    }

    #[test]
    fn test_convert_graphql_pr_to_pr_info_with_no_author() {
        let mut graphql_pr = create_test_graphql_pr();
        graphql_pr.author = None;

        let repo = Repo::new("owner", "repo").unwrap();
        let result = convert_graphql_pr_to_pr_info(graphql_pr, repo);
        assert!(result.is_ok());

        let pr_info = result.unwrap();
        assert_eq!(pr_info.author_login, "Unknown");
        assert_eq!(pr_info.author_simple_name, "Unknown");
    }

    #[test]
    fn test_convert_graphql_pr_to_pr_info_with_no_checks() {
        let mut graphql_pr = create_test_graphql_pr();
        graphql_pr.status_check_rollup = None;

        let repo = Repo::new("owner", "repo").unwrap();
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

        let repo = Repo::new("owner", "repo").unwrap();
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

        let repo = Repo::new("owner", "repo").unwrap();
        let result = convert_graphql_pr_to_pr_info(graphql_pr, repo);
        assert!(result.is_ok());

        let pr_info = result.unwrap();
        assert_eq!(pr_info.checks.len(), 1);
        assert!(pr_info.checks[0].url.is_none());
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
