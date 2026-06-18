use chrono::{DateTime, Utc};

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
    fetch_pull_requests_at(request, forge, Utc::now()).await
}

pub async fn fetch_pull_requests_at<F>(
    request: &QuerySpec,
    forge: &F,
    now: DateTime<Utc>,
) -> anyhow::Result<QueryResult>
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
        generate_executable_actions(&filtered_prs, &request.action_policy, now);

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

#[cfg(test)]
mod tests {
    use std::{sync::Mutex, time::Duration};

    use async_trait::async_trait;
    use chrono::{TimeZone, Utc};

    use super::*;
    use crate::{
        filters::AuthorPost,
        search::RepoSearch,
        types::{
            ActionPolicy, CommentAction, FetchCriteria, PrAction, PrState, Repo, SearchCriterion,
            SelectionPolicy,
        },
    };

    struct RecordingForge {
        prs: Vec<PullRequest>,
        seen_plan: Mutex<Option<FetchPlan>>,
    }

    impl RecordingForge {
        fn new(prs: Vec<PullRequest>) -> Self {
            Self {
                prs,
                seen_plan: Mutex::new(None),
            }
        }

        fn seen_plan(&self) -> Option<FetchPlan> {
            self.seen_plan.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl Forge for RecordingForge {
        async fn fetch_pull_requests(&self, plan: &FetchPlan) -> anyhow::Result<Vec<PullRequest>> {
            *self.seen_plan.lock().unwrap() = Some(plan.clone());
            Ok(self.prs.clone())
        }
    }

    fn repo() -> Repo {
        Repo::new("owner", "repo").unwrap()
    }

    fn pr(number: u64, author: &str, labels: &[&str]) -> PullRequest {
        PullRequest {
            repo: repo(),
            number,
            title: format!("PR {number}"),
            author_login: author.to_string(),
            author_simple_name: author.to_string(),
            url: format!("https://github.com/owner/repo/pull/{number}"),
            labels: labels.iter().map(|label| label.to_string()).collect(),
            created_at: Utc.with_ymd_and_hms(2026, 5, 29, 12, 0, 0).unwrap(),
            base_branch: "main".to_string(),
            commit_count: 1,
            is_draft: false,
            state: PrState::Open,
            checks: vec![],
            recent_comments: vec![],
        }
    }

    fn request() -> QuerySpec {
        QuerySpec {
            fetch: FetchCriteria {
                repos: vec![repo()],
                prs: vec![],
                query: None,
                limit: 20,
                search_criteria: vec![SearchCriterion::MissingLabel("lgtm".to_string())],
            },
            selection: SelectionPolicy {
                exclude: vec![],
                post_filters: vec![Box::new(AuthorPost::new().with_value("alice"))],
            },
            action_policy: ActionPolicy {
                actions: vec![PrAction::comment(CommentAction::Lgtm), PrAction::Close],
                throttle: None,
                history_max_age: Duration::from_secs(3600),
                history_max_comments: 10,
                commit_limit: 10,
            },
        }
    }

    #[tokio::test]
    async fn fetch_pull_requests_at_passes_plan_to_forge_then_filters_and_plans() {
        let forge = RecordingForge::new(vec![
            pr(1, "alice", &[]),
            pr(2, "bob", &[]),
            pr(3, "alice", &["lgtm"]),
        ]);
        let now = Utc.with_ymd_and_hms(2026, 5, 29, 12, 0, 0).unwrap();

        let result = fetch_pull_requests_at(&request(), &forge, now)
            .await
            .unwrap();

        assert_eq!(
            forge.seen_plan(),
            Some(FetchPlan::RepositorySearches(vec![RepoSearch {
                repo: repo(),
                criteria: vec![SearchCriterion::MissingLabel("lgtm".to_string())],
                limit: 20,
            }]))
        );
        assert_eq!(
            result
                .filtered_prs
                .iter()
                .map(|pr| pr.number)
                .collect::<Vec<_>>(),
            vec![1]
        );
        assert_eq!(
            result
                .executable_actions
                .iter()
                .map(|task| (task.pr_info.number, task.action.clone()))
                .collect::<Vec<_>>(),
            vec![
                (1, PrAction::comment(CommentAction::Lgtm)),
                (1, PrAction::Close),
            ]
        );
    }

    #[tokio::test]
    async fn fetch_pull_requests_at_errors_before_forge_without_fetch_criteria() {
        let mut request = request();
        request.fetch.repos.clear();
        request.fetch.search_criteria.clear();
        let forge = RecordingForge::new(vec![pr(1, "alice", &[])]);
        let now = Utc.with_ymd_and_hms(2026, 5, 29, 12, 0, 0).unwrap();

        let err = fetch_pull_requests_at(&request, &forge, now)
            .await
            .unwrap_err();

        assert_eq!(
            err.to_string(),
            "Query is required when not fetching specific PRs"
        );
        assert_eq!(forge.seen_plan(), None);
    }
}
