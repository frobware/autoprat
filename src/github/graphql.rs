//! GitHub GraphQL protocol layer: the outgoing query builder and the
//! incoming wire types with their deserialisers.
//!
//! Everything here is IO-free (sans-IO) -- no network, process, or
//! environment access -- but not domain-neutral: these types model
//! GitHub's external GraphQL contract. The IO that drives them lives in
//! the parent [`super`] module; the wire -> domain conversion lives in
//! [`super::convert`]. A different forge (e.g. GitLab) would have its
//! own equivalent of this module and reuse none of it; only the domain
//! model it converges on is shared.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use octocrab::models::{StatusState, workflows::Conclusion};
use serde::{Deserialize, Deserializer};
use url::Url;

/// Simple GraphQL query builder that eliminates brittle JSON manipulation
/// while maintaining clean type boundaries - only the GitHub adapter knows
/// about GraphQL.
pub(crate) struct GraphQLQueryBuilder {
    query: String,
    variables: HashMap<String, serde_json::Value>,
}

impl GraphQLQueryBuilder {
    /// Create a new query builder for searching pull requests
    pub(crate) fn search_pull_requests() -> Self {
        Self {
            query: include_str!("search_prs.graphql").to_string(),
            variables: HashMap::new(),
        }
    }

    /// Set the search query string
    pub(crate) fn with_search_query(mut self, query: &str) -> Self {
        self.variables.insert("query".to_string(), query.into());
        self
    }

    pub(crate) fn with_after_cursor(mut self, cursor: Option<String>) -> Self {
        self.variables.insert(
            "after".to_string(),
            cursor.map_or(serde_json::Value::Null, |c| c.into()),
        );
        self
    }

    pub(crate) fn build(self) -> serde_json::Value {
        serde_json::json!({
            "query": self.query,
            "variables": self.variables
        })
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
pub(crate) enum ActorType {
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
pub(crate) enum GraphQLCheckRunStatus {
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
pub(crate) enum GraphQLStatusContext {
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
pub(crate) struct GraphQLResponse {
    pub(crate) data: SearchData,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SearchData {
    pub(crate) search: SearchResults,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SearchResults {
    pub(crate) nodes: Vec<GraphQLPullRequest>,
    pub(crate) page_info: PageInfo,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PageInfo {
    pub(crate) has_next_page: bool,
    pub(crate) end_cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GraphQLPullRequest {
    pub(crate) number: u64,
    pub(crate) title: String,
    pub(crate) url: Url,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) base_ref_name: Option<String>,
    pub(crate) commits: GraphQLCommitConnection,
    pub(crate) author: Option<GraphQLAuthor>,
    pub(crate) labels: GraphQLLabelConnection,
    pub(crate) status_check_rollup: Option<GraphQLStatusCheckRollup>,
    pub(crate) comments: GraphQLCommentConnection,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GraphQLCommitConnection {
    pub(crate) total_count: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GraphQLAuthor {
    pub(crate) login: String,
    #[serde(rename = "__typename")]
    pub(crate) actor_type: ActorType,
}

impl GraphQLAuthor {
    pub(crate) fn search_format(&self) -> String {
        if self.actor_type.is_bot() {
            format!("app/{}", self.login)
        } else {
            self.login.clone()
        }
    }

    pub(crate) fn display_format(&self) -> String {
        if self.actor_type.is_bot() {
            format!("{}[bot]", self.login)
        } else {
            self.login.clone()
        }
    }

    pub(crate) fn simple_name(&self) -> String {
        self.login.clone()
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct GraphQLLabelConnection {
    pub(crate) nodes: Vec<GraphQLLabel>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GraphQLLabel {
    pub(crate) name: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GraphQLStatusCheckRollup {
    pub(crate) contexts: GraphQLStatusContextConnection,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GraphQLStatusContextConnection {
    pub(crate) nodes: Vec<GraphQLStatusContext>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GraphQLCommentConnection {
    pub(crate) nodes: Vec<GraphQLComment>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GraphQLComment {
    pub(crate) body: String,
    pub(crate) created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
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
}
