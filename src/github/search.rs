//! GitHub search query compilation.
//!
//! The core carries forge-neutral criteria. This module owns the GitHub
//! search syntax used to narrow those criteria server-side.

use crate::types::{Repo, SearchCriterion};

pub(crate) fn build_repo_search_query(repo: &Repo, criteria: &[SearchCriterion]) -> String {
    let mut parts = Vec::with_capacity(criteria.len() + 4);

    parts.push(format!("repo:{repo}"));
    for criterion in criteria {
        apply_criterion(criterion, &mut parts);
    }
    parts.push("type:pr".to_string());
    parts.push("state:open".to_string());
    parts.push("sort:created-asc".to_string());

    parts.join(" ")
}

pub(crate) fn format_user_query(query: &str) -> String {
    let mut final_query = query.to_string();

    if !final_query.contains("is:pr") {
        final_query = format!("{final_query} is:pr");
    }

    if !final_query.contains("is:open") && !final_query.contains("is:closed") {
        final_query = format!("{final_query} is:open");
    }

    final_query
}

fn apply_criterion(criterion: &SearchCriterion, terms: &mut Vec<String>) {
    match criterion {
        SearchCriterion::MissingLabel(label) => terms.push(format!("-label:{label}")),
        SearchCriterion::PresentLabel(label) => terms.push(format!("label:{label}")),
        SearchCriterion::BaseBranch(branch) => terms.push(format!("base:{branch}")),
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;
    use crate::types::PullRequest;

    fn repo() -> Repo {
        Repo::new("owner", "repo").unwrap()
    }

    fn pr(labels: &[&str], base_branch: &str) -> PullRequest {
        PullRequest {
            repo: repo(),
            number: 123,
            title: "Test PR".to_string(),
            author_login: "alice".to_string(),
            author_simple_name: "alice".to_string(),
            url: "https://github.com/owner/repo/pull/123".to_string(),
            labels: labels.iter().map(|label| label.to_string()).collect(),
            created_at: Utc.with_ymd_and_hms(2026, 5, 29, 12, 0, 0).unwrap(),
            base_branch: base_branch.to_string(),
            commit_count: 1,
            checks: vec![],
            recent_comments: vec![],
        }
    }

    fn terms(criterion: &SearchCriterion) -> Vec<String> {
        let mut terms = Vec::new();
        apply_criterion(criterion, &mut terms);
        terms
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
                SearchCriterion::MissingLabel("approved".to_string()),
                SearchCriterion::PresentLabel("bug".to_string()),
                SearchCriterion::MissingLabel("wip".to_string()),
                SearchCriterion::BaseBranch("main".to_string()),
            ],
        );

        assert_eq!(
            query,
            "repo:owner/repo -label:approved label:bug -label:wip base:main type:pr state:open sort:created-asc"
        );
    }

    #[test]
    fn missing_label_search_term_matches_local_predicate() {
        let criterion = SearchCriterion::MissingLabel("approved".to_string());

        assert_eq!(terms(&criterion), vec!["-label:approved"]);
        assert!(criterion.matches(&pr(&[], "main")));
        assert!(!criterion.matches(&pr(&["approved"], "main")));
    }

    #[test]
    fn present_label_search_term_matches_local_predicate() {
        let criterion = SearchCriterion::PresentLabel("bug".to_string());

        assert_eq!(terms(&criterion), vec!["label:bug"]);
        assert!(criterion.matches(&pr(&["bug"], "main")));
        assert!(!criterion.matches(&pr(&["feature"], "main")));
    }

    #[test]
    fn base_branch_search_term_matches_local_predicate() {
        let criterion = SearchCriterion::BaseBranch("release-1.0".to_string());

        assert_eq!(terms(&criterion), vec!["base:release-1.0"]);
        assert!(criterion.matches(&pr(&[], "release-1.0")));
        assert!(!criterion.matches(&pr(&[], "main")));
    }
}
