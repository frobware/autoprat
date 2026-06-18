//! GitHub command rendering for planned pull-request actions.
//!
//! The action intent is forge-neutral; this module renders that intent as
//! GitHub CLI commands.

use crate::{
    render::ActionRenderer,
    types::{PrAction, PullRequest},
};

#[derive(Debug, Default, Clone, Copy)]
pub struct GhCliRenderer;

impl ActionRenderer for GhCliRenderer {
    fn render(&self, pr: &PullRequest, action: &PrAction) -> String {
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
            author_simple_name: "alice".to_string(),
            url: "https://github.com/owner/repo/pull/123".to_string(),
            labels: vec![],
            created_at: Utc.with_ymd_and_hms(2026, 5, 29, 12, 0, 0).unwrap(),
            base_branch: "main".to_string(),
            commit_count: 1,
            is_draft: false,
            checks: vec![],
            recent_comments: vec![],
        }
    }

    #[test]
    fn renders_planned_grouped_comment_body_without_redeciding() {
        let command = GhCliRenderer.render(
            &pr(),
            &PrAction::comments(vec![
                CommentAction::Approve,
                CommentAction::Custom("Please review".to_string()),
            ])
            .unwrap(),
        );

        assert_eq!(
            command,
            "gh pr comment https://github.com/owner/repo/pull/123 --body $'/approve\\nPlease review'"
        );
    }

    #[test]
    fn renders_close_and_merge_actions() {
        assert_eq!(
            GhCliRenderer.render(&pr(), &PrAction::Close),
            "gh pr close https://github.com/owner/repo/pull/123"
        );
        assert_eq!(
            GhCliRenderer.render(&pr(), &PrAction::Merge),
            "gh pr merge --merge https://github.com/owner/repo/pull/123"
        );
    }
}
