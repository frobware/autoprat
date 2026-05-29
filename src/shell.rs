use std::io::Write;

use anyhow::Result;

use crate::types::{PrAction, PullRequest, Task};

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

pub fn format_shell_command_line(task: &Task) -> String {
    let pr = &task.pr_info;
    let command = format_shell_command(&task.action, pr);
    format!("{command} # [{}] {}", pr.base_branch, pr.title)
}

pub fn write_shell_commands<W: Write>(actions: &[Task], writer: &mut W) -> Result<()> {
    for action in actions {
        writeln!(writer, "{}", format_shell_command_line(action))?;
    }
    Ok(())
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

    #[test]
    fn writes_shell_commands_with_trailing_branch_and_title_comment() {
        let tasks = vec![Task {
            pr_info: pr(),
            action: PrAction::comment(CommentAction::Custom("hello".to_string())),
        }];
        let mut output = Vec::new();

        write_shell_commands(&tasks, &mut output).unwrap();

        assert_eq!(
            String::from_utf8(output).unwrap(),
            "gh pr comment https://github.com/owner/repo/pull/123 --body \"hello\" # [main] Test PR\n"
        );
    }
}
