use crate::{
    pr_selector::PrIdentifier,
    types::{FetchCriteria, Repo, SearchFilter},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoSearch {
    pub repo: Repo,
    pub query: String,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchPlan {
    SpecificPullRequests(Vec<PrIdentifier>),
    UserSearch { query: String, limit: usize },
    RepositorySearches(Vec<RepoSearch>),
}

impl FetchPlan {
    pub fn from_criteria(criteria: &FetchCriteria) -> Option<Self> {
        if !criteria.prs.is_empty() {
            return Some(Self::SpecificPullRequests(criteria.prs.clone()));
        }

        if let Some(query) = &criteria.query {
            return Some(Self::UserSearch {
                query: query.clone(),
                limit: criteria.limit,
            });
        }

        if !criteria.repos.is_empty() {
            return Some(Self::RepositorySearches(
                criteria
                    .repos
                    .iter()
                    .map(|repo| RepoSearch {
                        repo: repo.clone(),
                        query: build_repo_search_query(repo, &criteria.search_filters),
                        limit: criteria.limit,
                    })
                    .collect(),
            ));
        }

        None
    }
}

pub fn build_repo_search_query(
    repo: &Repo,
    search_filters: &[Box<dyn SearchFilter + Send + Sync>],
) -> String {
    let mut parts = Vec::with_capacity(search_filters.len() + 4);

    parts.push(format!("repo:{repo}"));
    for sf in search_filters {
        sf.apply(&mut parts);
    }
    parts.push("type:pr".to_string());
    parts.push("state:open".to_string());
    parts.push("sort:created-asc".to_string());

    parts.join(" ")
}

pub fn format_user_query(query: &str) -> String {
    let mut final_query = query.to_string();

    if !final_query.contains("is:pr") {
        final_query = format!("{final_query} is:pr");
    }

    if !final_query.contains("is:open") && !final_query.contains("is:closed") {
        final_query = format!("{final_query} is:open");
    }

    final_query
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filters::{LabelSearch, NeedsApproveSearch};

    fn empty_criteria() -> FetchCriteria {
        FetchCriteria {
            repos: vec![],
            prs: vec![],
            query: None,
            limit: 30,
            search_filters: vec![],
        }
    }

    fn repo() -> Repo {
        Repo::new("owner", "repo").unwrap()
    }

    #[test]
    fn user_query_defaults_to_open_prs() {
        assert_eq!(
            format_user_query("author:alice"),
            "author:alice is:pr is:open"
        );
    }

    #[test]
    fn user_query_keeps_explicit_state() {
        assert_eq!(
            format_user_query("repo:o/r is:closed"),
            "repo:o/r is:closed is:pr"
        );
    }

    #[test]
    fn repo_search_query_includes_repo_filters_and_fixed_terms() {
        let query = build_repo_search_query(
            &repo(),
            &[
                Box::new(NeedsApproveSearch),
                Box::new(LabelSearch {
                    labels: vec!["bug".to_string(), "-wip".to_string()],
                }),
            ],
        );

        assert_eq!(
            query,
            "repo:owner/repo -label:approved label:bug -label:wip type:pr state:open sort:created-asc"
        );
    }

    #[test]
    fn fetch_plan_prefers_specific_pull_requests_over_query_and_repos() {
        let mut criteria = empty_criteria();
        criteria.repos = vec![repo()];
        criteria.query = Some("author:alice is:pr".to_string());
        criteria.prs = vec![PrIdentifier::new(repo(), 123)];

        assert_eq!(
            FetchPlan::from_criteria(&criteria),
            Some(FetchPlan::SpecificPullRequests(vec![PrIdentifier::new(
                repo(),
                123
            )]))
        );
    }

    #[test]
    fn fetch_plan_prefers_user_query_over_repos() {
        let mut criteria = empty_criteria();
        criteria.repos = vec![repo()];
        criteria.query = Some("author:alice is:pr".to_string());

        assert_eq!(
            FetchPlan::from_criteria(&criteria),
            Some(FetchPlan::UserSearch {
                query: "author:alice is:pr".to_string(),
                limit: 30,
            })
        );
    }

    #[test]
    fn fetch_plan_builds_repository_searches() {
        let mut criteria = empty_criteria();
        criteria.limit = 50;
        criteria.repos = vec![repo()];
        criteria.search_filters = vec![Box::new(NeedsApproveSearch)];

        assert_eq!(
            FetchPlan::from_criteria(&criteria),
            Some(FetchPlan::RepositorySearches(vec![RepoSearch {
                repo: repo(),
                query: "repo:owner/repo -label:approved type:pr state:open sort:created-asc"
                    .to_string(),
                limit: 50,
            }]))
        );
    }

    #[test]
    fn fetch_plan_is_absent_without_fetch_criteria() {
        assert_eq!(FetchPlan::from_criteria(&empty_criteria()), None);
    }
}
