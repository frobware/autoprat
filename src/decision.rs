use std::time::Duration;

use chrono::{DateTime, Utc};

use crate::types::{
    ActionPolicy, CommentAction, FetchCriteria, PrAction, PullRequest, SelectionPolicy, Task,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitLimitOffender {
    pub url: String,
    pub commit_count: u64,
}

pub fn pull_request_matches(
    pr: &PullRequest,
    fetch: &FetchCriteria,
    selection: &SelectionPolicy,
) -> bool {
    if selection
        .exclude
        .iter()
        .any(|id| pr.repo == id.repo && pr.number == id.number)
    {
        return false;
    }

    (fetch.prs.is_empty()
        || fetch
            .prs
            .iter()
            .any(|id| pr.repo == id.repo && pr.number == id.number))
        && fetch
            .search_criteria
            .iter()
            .all(|criterion| criterion.matches(pr))
        && selection.post_filters.iter().all(|pf| pf.matches(pr))
}

fn was_comment_posted_recently(
    pr: &PullRequest,
    comment_body: &str,
    throttle_duration: Duration,
    now: DateTime<Utc>,
) -> bool {
    let cutoff_time = now - chrono::Duration::seconds(throttle_duration.as_secs() as i64);
    let target_command = comment_body.trim();

    pr.recent_comments.iter().any(|comment| {
        comment.created_at > cutoff_time
            && comment
                .body
                .lines()
                .any(|line| line.trim() == target_command)
    })
}

fn was_comment_posted_in_history(
    pr: &PullRequest,
    comment_body: &str,
    max_comments_to_check: usize,
    max_age: Duration,
    now: DateTime<Utc>,
) -> bool {
    let target_command = comment_body.trim();
    let cutoff_time = now - chrono::Duration::seconds(max_age.as_secs() as i64);

    pr.recent_comments
        .iter()
        .rev()
        .take(max_comments_to_check)
        .any(|comment| {
            comment.created_at > cutoff_time
                && comment
                    .body
                    .lines()
                    .any(|line| line.trim() == target_command)
        })
}

fn comment_should_execute(
    action: &CommentAction,
    pr: &PullRequest,
    history_max_comments: usize,
    history_max_age: Duration,
    throttle: Option<Duration>,
    now: DateTime<Utc>,
) -> bool {
    if !action.only_if(pr) {
        return false;
    }

    let body = action.body();
    if was_comment_posted_in_history(pr, body, history_max_comments, history_max_age, now) {
        return false;
    }

    if let Some(duration) = throttle
        && was_comment_posted_recently(pr, body, duration, now)
    {
        return false;
    }

    true
}

pub fn plan_executable_action(
    action: &PrAction,
    pr: &PullRequest,
    history_max_comments: usize,
    history_max_age: Duration,
    throttle: Option<Duration>,
    now: DateTime<Utc>,
) -> Option<PrAction> {
    match action {
        PrAction::Comment(action) => comment_should_execute(
            action,
            pr,
            history_max_comments,
            history_max_age,
            throttle,
            now,
        )
        .then(|| PrAction::Comment(action.clone())),
        PrAction::GroupedComment(actions) => {
            let executable_actions = actions
                .iter()
                .filter(|action| {
                    comment_should_execute(
                        action,
                        pr,
                        history_max_comments,
                        history_max_age,
                        throttle,
                        now,
                    )
                })
                .cloned()
                .collect::<Vec<_>>();

            PrAction::comments(executable_actions)
        }
        PrAction::Close | PrAction::Merge => Some(action.clone()),
    }
}

pub fn generate_executable_actions(
    filtered_prs: &[PullRequest],
    policy: &ActionPolicy,
    now: DateTime<Utc>,
) -> Vec<Task> {
    let mut executable_actions = Vec::with_capacity(filtered_prs.len() * policy.actions.len());

    for pr in filtered_prs {
        for action in &policy.actions {
            if let Some(action) = plan_executable_action(
                action,
                pr,
                policy.history_max_comments,
                policy.history_max_age,
                policy.throttle,
                now,
            ) {
                executable_actions.push(Task {
                    pr_info: pr.clone(),
                    action,
                });
            }
        }
    }

    executable_actions
}

pub fn commit_limit_offenders(tasks: &[Task], limit: u64) -> Vec<CommitLimitOffender> {
    let mut offenders = tasks
        .iter()
        .filter(|task| task.pr_info.commit_count > limit)
        .map(|task| CommitLimitOffender {
            url: task.pr_info.url.clone(),
            commit_count: task.pr_info.commit_count,
        })
        .collect::<Vec<_>>();
    offenders.sort_by(|a, b| a.url.cmp(&b.url));
    offenders.dedup_by(|a, b| a.url == b.url);
    offenders
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use chrono::{TimeZone, Utc};

    use super::*;
    use crate::{
        filters::AuthorPost,
        pr_selector::PrIdentifier,
        types::{CommentInfo, PullRequest, Repo, SearchCriterion},
    };

    fn pr_with_comments(recent_comments: Vec<CommentInfo>) -> PullRequest {
        PullRequest {
            repo: Repo::new("owner", "repo").unwrap(),
            number: 123,
            title: "Test PR".to_string(),
            author_login: "alice".to_string(),
            author_simple_name: "alice".to_string(),
            url: "https://github.com/owner/repo/pull/123".to_string(),
            labels: vec![],
            created_at: Utc.with_ymd_and_hms(2026, 5, 29, 12, 0, 0).unwrap(),
            base_branch: "main".to_string(),
            commit_count: 1,
            is_draft: false,
            checks: vec![],
            recent_comments,
        }
    }

    fn pr_with_state(
        number: u64,
        labels: &[&str],
        recent_comments: Vec<CommentInfo>,
    ) -> PullRequest {
        let mut pr = pr_with_comments(recent_comments);
        pr.number = number;
        pr.url = format!("https://github.com/owner/repo/pull/{number}");
        pr.labels = labels.iter().map(|label| label.to_string()).collect();
        pr
    }

    fn planned_actions(tasks: &[Task]) -> Vec<(u64, PrAction)> {
        tasks
            .iter()
            .map(|task| (task.pr_info.number, task.action.clone()))
            .collect()
    }

    #[test]
    fn grouped_comment_prunes_history_suppressed_sub_actions() {
        let now = Utc.with_ymd_and_hms(2026, 5, 29, 12, 0, 0).unwrap();
        let pr = pr_with_comments(vec![CommentInfo {
            body: "/lgtm".to_string(),
            created_at: now - chrono::Duration::minutes(5),
        }]);
        let action = PrAction::comments(vec![CommentAction::Approve, CommentAction::Lgtm]).unwrap();

        let planned =
            plan_executable_action(&action, &pr, 10, Duration::from_secs(3600), None, now);

        assert_eq!(planned, Some(PrAction::comment(CommentAction::Approve)));
    }

    #[test]
    fn grouped_comment_is_suppressed_when_all_sub_actions_are_suppressed() {
        let now = Utc.with_ymd_and_hms(2026, 5, 29, 12, 0, 0).unwrap();
        let pr = pr_with_comments(vec![CommentInfo {
            body: "/lgtm".to_string(),
            created_at: now - chrono::Duration::minutes(5),
        }]);
        let action = PrAction::comment(CommentAction::Lgtm);

        let planned =
            plan_executable_action(&action, &pr, 10, Duration::from_secs(3600), None, now);

        assert_eq!(planned, None);
    }

    #[test]
    fn grouped_comment_prunes_throttled_custom_sub_actions() {
        let now = Utc.with_ymd_and_hms(2026, 5, 29, 12, 0, 0).unwrap();
        let pr = pr_with_comments(vec![CommentInfo {
            body: "Needs attention".to_string(),
            created_at: now - chrono::Duration::minutes(2),
        }]);
        let action = PrAction::comments(vec![
            CommentAction::Custom("Needs attention".to_string()),
            CommentAction::Custom("Please review".to_string()),
        ])
        .unwrap();

        let planned = plan_executable_action(
            &action,
            &pr,
            10,
            Duration::from_secs(3600),
            Some(Duration::from_secs(300)),
            now,
        );

        assert_eq!(
            planned,
            Some(PrAction::comment(CommentAction::Custom(
                "Please review".to_string()
            )))
        );
    }

    #[test]
    fn pull_request_matches_combines_targets_excludes_and_filters() {
        let mut pr = pr_with_comments(vec![]);
        pr.number = 124;
        pr.author_login = "alice".to_string();

        let fetch = FetchCriteria {
            repos: vec![],
            prs: vec![PrIdentifier {
                repo: pr.repo.clone(),
                number: 124,
            }],
            query: None,
            limit: 100,
            search_criteria: vec![SearchCriterion::MissingLabel("lgtm".to_string())],
        };
        let selection = SelectionPolicy {
            exclude: vec![],
            post_filters: vec![Box::new(AuthorPost::new().with_value("alice"))],
        };

        assert!(pull_request_matches(&pr, &fetch, &selection));

        let excluded = SelectionPolicy {
            exclude: vec![PrIdentifier {
                repo: pr.repo.clone(),
                number: 124,
            }],
            post_filters: vec![Box::new(AuthorPost::new().with_value("alice"))],
        };
        assert!(!pull_request_matches(&pr, &fetch, &excluded));

        pr.labels.push("lgtm".to_string());
        assert!(!pull_request_matches(&pr, &fetch, &selection));
    }

    #[test]
    fn generate_executable_actions_plans_core_behaviour_without_edges() {
        let now = Utc.with_ymd_and_hms(2026, 5, 29, 12, 0, 0).unwrap();
        let policy = ActionPolicy {
            actions: vec![
                PrAction::comments(vec![CommentAction::Approve, CommentAction::Lgtm]).unwrap(),
                PrAction::comment(CommentAction::Custom("Needs attention".to_string())),
                PrAction::Close,
            ],
            throttle: Some(Duration::from_secs(300)),
            history_max_age: Duration::from_secs(3600),
            history_max_comments: 10,
            commit_limit: 1,
        };
        let prs = vec![
            pr_with_state(1, &[], vec![]),
            pr_with_state(
                2,
                &["approved"],
                vec![CommentInfo {
                    body: "Needs attention".to_string(),
                    created_at: now - chrono::Duration::minutes(1),
                }],
            ),
            pr_with_state(
                3,
                &["approved", "lgtm"],
                vec![CommentInfo {
                    body: "Needs attention".to_string(),
                    created_at: now - chrono::Duration::minutes(1),
                }],
            ),
        ];

        let tasks = generate_executable_actions(&prs, &policy, now);

        assert_eq!(
            planned_actions(&tasks),
            vec![
                (
                    1,
                    PrAction::GroupedComment(vec![CommentAction::Approve, CommentAction::Lgtm])
                ),
                (
                    1,
                    PrAction::comment(CommentAction::Custom("Needs attention".to_string()))
                ),
                (1, PrAction::Close),
                (2, PrAction::comment(CommentAction::Lgtm)),
                (2, PrAction::Close),
                (3, PrAction::Close),
            ]
        );
    }

    #[test]
    fn commit_limit_offenders_returns_sorted_unique_offenders() {
        let mut over = pr_with_comments(vec![]);
        over.url = "https://github.com/owner/repo/pull/2".to_string();
        over.commit_count = 3;

        let mut ok = pr_with_comments(vec![]);
        ok.url = "https://github.com/owner/repo/pull/1".to_string();
        ok.commit_count = 1;

        let tasks = vec![
            Task {
                pr_info: over.clone(),
                action: PrAction::Close,
            },
            Task {
                pr_info: ok,
                action: PrAction::Close,
            },
            Task {
                pr_info: over,
                action: PrAction::Merge,
            },
        ];

        assert_eq!(
            commit_limit_offenders(&tasks, 1),
            vec![CommitLimitOffender {
                url: "https://github.com/owner/repo/pull/2".to_string(),
                commit_count: 3,
            }]
        );
    }
}
