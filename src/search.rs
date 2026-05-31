use crate::{
    pr_selector::PrIdentifier,
    types::{FetchCriteria, Repo, SearchCriterion},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoSearch {
    pub repo: Repo,
    pub criteria: Vec<SearchCriterion>,
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
                        criteria: criteria.search_criteria.clone(),
                        limit: criteria.limit,
                    })
                    .collect(),
            ));
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_criteria() -> FetchCriteria {
        FetchCriteria {
            repos: vec![],
            prs: vec![],
            query: None,
            limit: 30,
            search_criteria: vec![],
        }
    }

    fn repo() -> Repo {
        Repo::new("owner", "repo").unwrap()
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
        criteria.search_criteria = vec![SearchCriterion::MissingLabel("lgtm".to_string())];

        assert_eq!(
            FetchPlan::from_criteria(&criteria),
            Some(FetchPlan::RepositorySearches(vec![RepoSearch {
                repo: repo(),
                criteria: vec![SearchCriterion::MissingLabel("lgtm".to_string())],
                limit: 50,
            }]))
        );
    }

    #[test]
    fn fetch_plan_is_absent_without_fetch_criteria() {
        assert_eq!(FetchPlan::from_criteria(&empty_criteria()), None);
    }
}
