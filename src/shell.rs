use crate::types::{PrAction, PullRequest};

pub fn format_shell_command(action: &PrAction, pr: &PullRequest) -> String {
    match action {
        PrAction::Close => format!("gh pr close {}", pr.url),
        PrAction::Merge => format!("gh pr merge --merge {}", pr.url),
        PrAction::Comment(action) => {
            format!("gh pr comment {} --body \"{}\"", pr.url, action.body())
        }
        PrAction::GroupedComment(actions) => {
            let body = actions
                .iter()
                .map(|action| action.body())
                .collect::<Vec<_>>()
                .join("\\n");
            format!("gh pr comment {} --body $'{body}'", pr.url)
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;
    use crate::types::{CommentAction, Repo};

    fn pr() -> PullRequest {
        PullRequest {
            repo: Repo::new("owner", "repo").unwrap(),
            number: 123,
            title: "Test PR".to_string(),
            author_login: "alice".to_string(),
            author_search_format: "alice".to_string(),
            author_simple_name: "alice".to_string(),
            url: "https://github.com/owner/repo/pull/123".to_string(),
            labels: vec![],
            created_at: Utc.with_ymd_and_hms(2026, 5, 29, 12, 0, 0).unwrap(),
            base_branch: "main".to_string(),
            commit_count: 1,
            checks: vec![],
            recent_comments: vec![],
        }
    }

    #[test]
    fn renders_planned_grouped_comment_body_without_redeciding() {
        let command = format_shell_command(
            &PrAction::comments(vec![
                CommentAction::Approve,
                CommentAction::Custom("Please review".to_string()),
            ])
            .unwrap(),
            &pr(),
        );

        assert_eq!(
            command,
            "gh pr comment https://github.com/owner/repo/pull/123 --body $'/approve\\nPlease review'"
        );
    }

    #[test]
    fn renders_close_and_merge_actions() {
        assert_eq!(
            format_shell_command(&PrAction::Close, &pr()),
            "gh pr close https://github.com/owner/repo/pull/123"
        );
        assert_eq!(
            format_shell_command(&PrAction::Merge, &pr()),
            "gh pr merge --merge https://github.com/owner/repo/pull/123"
        );
    }
}
