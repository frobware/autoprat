use anyhow::{Context, Result};
use chrono::Utc;

use crate::{display::parse_duration, search::has_label, types::*};

pub fn should_throttle_comment(
    comment_text: &str,
    recent_comments: &[CommentInfo],
    throttle_duration: Option<chrono::Duration>,
) -> bool {
    if let Some(duration) = throttle_duration {
        let now = Utc::now();
        let cutoff_time = now - duration;

        for comment in recent_comments {
            if comment.created_at > cutoff_time && comment.body.trim() == comment_text.trim() {
                return true;
            }
        }
    }

    false
}

/// Predicate function that determines if an action should be applied to a PR.
type ActionCondition = fn(&SimplePR) -> bool;

/// Specification for a bot action: enabled flag, command, and application
/// condition.
type ActionSpec = (bool, BotCommand, ActionCondition);

pub fn generate_action_commands(prs: &[PrInfo], cli: &crate::Cli) -> Result<Vec<String>> {
    let mut commands = Vec::new();

    let throttle_duration = if let Some(throttle_str) = &cli.throttle {
        Some(
            parse_duration(throttle_str)
                .with_context(|| format!("Invalid throttle duration: '{}'", throttle_str))?,
        )
    } else {
        None
    };

    let actions_to_perform: &[ActionSpec] = &[
        (cli.approve, BotCommand::Approve, |pr: &SimplePR| {
            !has_label(pr, KnownLabel::Approved)
        }),
        (cli.lgtm, BotCommand::Lgtm, |pr: &SimplePR| {
            !has_label(pr, KnownLabel::Lgtm)
        }),
        (cli.ok_to_test, BotCommand::OkToTest, |pr: &SimplePR| {
            has_label(pr, KnownLabel::NeedsOkToTest)
        }),
        (cli.retest, BotCommand::Retest, |_pr: &SimplePR| true),
    ];

    for pr_info in prs {
        let repo_full = format!("{}/{}", pr_info.repo_owner, pr_info.repo_name);
        let pr_number = pr_info.pr.number;

        for (enabled, command, condition) in actions_to_perform {
            if *enabled
                && condition(&pr_info.pr)
                && !should_throttle_comment(
                    command.as_str(),
                    &pr_info.recent_comments,
                    throttle_duration,
                )
            {
                commands.push(format!(
                    "gh pr comment {} --repo {} --body \"{}\"",
                    pr_number,
                    repo_full,
                    command.as_str()
                ));
            }
        }

        for comment_text in &cli.comment {
            if !should_throttle_comment(comment_text, &pr_info.recent_comments, throttle_duration) {
                commands.push(format!(
                    "gh pr comment {} --repo {} --body \"{}\"",
                    pr_number, repo_full, comment_text
                ));
            }
        }

        // Handle close action separately since it doesn't post a comment
        if cli.close {
            commands.push(format!("gh pr close {} --repo {}", pr_number, repo_full));
        }
    }

    Ok(commands)
}

/// Determines whether any action commands are enabled in the CLI configuration.
pub fn has_action_commands(cli: &crate::Cli) -> bool {
    cli.approve || cli.lgtm || cli.ok_to_test || cli.close || cli.retest || !cli.comment.is_empty()
}

/// Outputs generated GitHub CLI commands to stdout for execution.
pub fn output_commands(commands: Vec<String>) {
    for command in commands {
        println!("{}", command);
    }
}
