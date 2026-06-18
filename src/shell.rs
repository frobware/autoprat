use std::io::Write;

use anyhow::Result;

use crate::{render::ActionRenderer, types::Task};

pub fn format_shell_command_line(renderer: &impl ActionRenderer, task: &Task) -> String {
    let pr = &task.pr_info;
    let command = renderer.render(pr, &task.action);
    format!("{command} # [{}] {}", pr.base_branch, pr.title)
}

pub fn write_shell_commands<W: Write>(
    renderer: &impl ActionRenderer,
    actions: &[Task],
    writer: &mut W,
) -> Result<()> {
    for action in actions {
        writeln!(writer, "{}", format_shell_command_line(renderer, action))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;
    use crate::types::{CommentAction, PrAction, PullRequest, Repo};

    #[derive(Debug, Default)]
    struct FakeRenderer;

    impl ActionRenderer for FakeRenderer {
        fn render(&self, pr: &PullRequest, action: &PrAction) -> String {
            format!("rendered:{}:{action:?}", pr.number)
        }
    }

    fn pr() -> PullRequest {
        PullRequest {
            repo: Repo::new("owner", "repo").unwrap(),
            number: 123,
            title: "Test PR".to_string(),
            author_login: "alice".to_string(),
            author_simple_name: "alice".to_string(),
            url: "https://example.test/owner/repo/pull/123".to_string(),
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
    fn writes_commands_from_the_supplied_renderer_with_context_comment() {
        let tasks = vec![Task {
            pr_info: pr(),
            action: PrAction::comment(CommentAction::Custom("hello".to_string())),
        }];
        let mut output = Vec::new();

        write_shell_commands(&FakeRenderer, &tasks, &mut output).unwrap();

        assert_eq!(
            String::from_utf8(output).unwrap(),
            "rendered:123:Comment(Custom(\"hello\")) # [main] Test PR\n"
        );
    }
}
