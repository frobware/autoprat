use anyhow::Result;
use octocrab::Octocrab;

use crate::{github::extract_repo_info_from_url, types::*};

pub fn create_graphql_query() -> serde_json::Value {
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

/// Converts a GraphQL status context into a unified CheckInfo structure.
pub fn convert_graphql_status_context(context: GraphQLStatusContext) -> CheckInfo {
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
pub fn convert_status_checks(rollup: Option<GraphQLStatusCheckRollup>) -> Vec<CheckInfo> {
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
pub fn convert_comments(comments: GraphQLCommentConnection) -> Vec<CommentInfo> {
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
pub fn convert_graphql_pr_to_pr_info(graphql_pr: GraphQLPullRequest) -> Result<PrInfo> {
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

// Simple function to fetch a single PR by search query
pub async fn fetch_single_pr_by_query(
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
