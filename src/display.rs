use std::io::{self, IsTerminal};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use chrono_humanize::HumanTime;
use octocrab::models::{StatusState, workflows::Conclusion};

use crate::{search::has_label, types::*};

pub fn parse_duration(duration_str: &str) -> Result<chrono::Duration> {
    let duration_str = duration_str.trim();

    if let Some(num_str) = duration_str.strip_suffix('m') {
        let minutes: i64 = num_str
            .parse()
            .with_context(|| format!("Invalid minutes value in duration: '{}'", duration_str))?;
        Ok(chrono::Duration::minutes(minutes))
    } else if let Some(num_str) = duration_str.strip_suffix('h') {
        let hours: i64 = num_str
            .parse()
            .with_context(|| format!("Invalid hours value in duration: '{}'", duration_str))?;
        Ok(chrono::Duration::hours(hours))
    } else if let Some(num_str) = duration_str.strip_suffix('s') {
        let seconds: i64 = num_str
            .parse()
            .with_context(|| format!("Invalid seconds value in duration: '{}'", duration_str))?;
        Ok(chrono::Duration::seconds(seconds))
    } else {
        // Try parsing as minutes if no suffix
        let minutes: i64 = duration_str.parse().with_context(|| {
            format!(
                "Invalid duration (expected format: 5m, 2h, 30s): '{}'",
                duration_str
            )
        })?;
        Ok(chrono::Duration::minutes(minutes))
    }
}

pub fn format_relative_time(created_at: DateTime<Utc>) -> String {
    HumanTime::from(created_at).to_string()
}

pub fn fetch_and_filter_logs(url: &str) -> Result<Vec<String>> {
    // Convert Prow CI URLs to direct log URLs
    let log_url = if url.contains("prow.ci.openshift.org/view/gs/") {
        url.replace("prow.ci.openshift.org/view/gs/", "storage.googleapis.com/") + "/build-log.txt"
    } else if url.contains("raw") || url.contains("storage.googleapis.com") {
        // Already a raw log URL
        url.to_string()
    } else {
        // Skip non-log URLs (e.g., GitHub comment URLs)
        if url.contains("#issuecomment") {
            return Ok(Vec::new());
        }
        // For other CI systems, we might not know how to get logs
        return Ok(Vec::new());
    };

    // Fetch the log content
    let response = match ureq::get(&log_url)
        .timeout(std::time::Duration::from_secs(10))
        .call()
    {
        Ok(resp) => resp,
        Err(_) => return Ok(Vec::new()),
    };

    if response.status() != 200 {
        return Ok(Vec::new());
    }

    let content = match response.into_string() {
        Ok(text) => text,
        Err(_) => return Ok(Vec::new()),
    };

    filter_error_lines_from_content(&content)
}

pub fn filter_error_lines_from_content(content: &str) -> Result<Vec<String>> {
    static ERROR_PATTERNS: std::sync::OnceLock<Vec<LogErrorPattern>> = std::sync::OnceLock::new();
    let error_patterns = ERROR_PATTERNS.get_or_init(LogErrorPattern::all_patterns);

    let mut error_lines = Vec::new();
    for line in content.lines() {
        // Skip empty lines and very long lines
        if line.trim().is_empty() || line.len() > 500 {
            continue;
        }

        let is_error = error_patterns.iter().any(|pattern| pattern.matches(line));

        // Also check for exit codes 1-9
        if is_error
            || (line.contains("exit code") && line.chars().any(|c| ('1'..='9').contains(&c)))
        {
            error_lines.push(line.trim().to_string());

            // Limit to 20 error lines
            if error_lines.len() >= 20 {
                error_lines.push("... (truncated)".to_string());
                break;
            }
        }
    }

    Ok(error_lines)
}

pub fn get_ci_status(checks: &[CheckInfo]) -> Result<CiStatus> {
    if checks.is_empty() {
        return Ok(CiStatus::Unknown);
    }

    let mut has_failure = false;
    let mut has_pending = false;
    let mut has_success = false;

    for check in checks {
        if let Some(conclusion) = &check.conclusion {
            match CiStatus::from_conclusion(conclusion)? {
                CiStatus::Success => has_success = true,
                CiStatus::Failing => has_failure = true,
                CiStatus::Pending => has_pending = true,
                CiStatus::Unknown => has_pending = true,
            }
        }

        if let Some(state) = &check.status_state {
            match CiStatus::from_status_state(state)? {
                CiStatus::Success => has_success = true,
                CiStatus::Failing => has_failure = true,
                CiStatus::Pending => has_pending = true,
                CiStatus::Unknown => has_pending = true,
            }
        }

        // If neither conclusion nor state is present, treat as pending
        if check.conclusion.is_none() && check.status_state.is_none() {
            has_pending = true;
        }
    }

    if has_failure {
        Ok(CiStatus::Failing)
    } else if has_pending {
        Ok(CiStatus::Pending)
    } else if has_success {
        Ok(CiStatus::Success)
    } else {
        Ok(CiStatus::Unknown)
    }
}

pub fn display_prs_table(prs: &[PrInfo]) -> Result<()> {
    if prs.is_empty() {
        println!("No pull requests found matching filters.");
        return Ok(());
    }

    // Column headers
    let headers = [
        "URL",
        "CI",
        "APP",
        "LGTM",
        "OK2TST",
        "HOLD",
        "AUTHOR",
        "CREATED AT",
        "TITLE",
    ];

    // Get terminal width only if stdout is a TTY
    let terminal_width = if io::stdout().is_terminal() {
        terminal_size::terminal_size()
            .map(|(w, _)| w.0 as usize)
            .unwrap_or(usize::MAX)
    } else {
        usize::MAX // No clamping when piping
    };

    // Prepare all rows with truncated titles
    let mut rows: Vec<Vec<String>> = Vec::new();

    for pr_info in prs {
        let pr = &pr_info.pr;
        let ci_status = get_ci_status(&pr_info.checks)?;
        let approved = if has_label(pr, KnownLabel::Approved) {
            "✓"
        } else {
            "✗"
        };
        let lgtm = if has_label(pr, KnownLabel::Lgtm) {
            "✓"
        } else {
            "✗"
        };
        let ok2test = if has_label(pr, KnownLabel::OkToTest) {
            "✓"
        } else {
            "✗"
        };
        let hold = if has_label(pr, KnownLabel::DoNotMergeHold) {
            "Y"
        } else {
            "N"
        };

        // Store title without truncation for now
        let title = pr.title.clone();

        rows.push(vec![
            pr.url.clone(),
            ci_status.to_string(),
            approved.to_string(),
            lgtm.to_string(),
            ok2test.to_string(),
            hold.to_string(),
            pr.author_simple_name.clone(),
            format_relative_time(pr.created_at),
            title,
        ]);
    }

    // Calculate column widths (excluding title column initially)
    let mut widths = headers.iter().map(|h| h.len()).collect::<Vec<_>>();

    for row in &rows {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(cell.len());
        }
    }

    // Calculate space used by all columns except title
    // Each column separator is 2 spaces
    let non_title_columns = headers.len() - 1;
    let separator_width = 2 * (non_title_columns - 1);
    let non_title_width: usize =
        widths[..non_title_columns].iter().sum::<usize>() + separator_width;

    // If we have a terminal width and non-title columns don't already exceed it
    if terminal_width != usize::MAX && non_title_width < terminal_width {
        // Calculate available space for title column
        let available_title_width = terminal_width - non_title_width - 2; // -2 for separator before title

        // Only clamp if we need to (some title exceeds available width)
        let max_title_width = rows
            .iter()
            .map(|row| row[non_title_columns].len())
            .max()
            .unwrap_or(0);

        if max_title_width > available_title_width && available_title_width > 3 {
            // Update title column width to available width
            widths[non_title_columns] = available_title_width;

            // Truncate titles that are too long
            for row in &mut rows {
                let title = &row[non_title_columns];
                if title.len() > available_title_width {
                    row[non_title_columns] = format!("{}...", &title[..available_title_width - 3]);
                }
            }
        }
    }

    // Print header
    for (i, header) in headers.iter().enumerate() {
        print!("{:<width$}", header, width = widths[i]);
        if i < headers.len() - 1 {
            print!("  ");
        }
    }
    println!();

    // Print separator
    for (i, &width) in widths.iter().enumerate() {
        print!("{}", "-".repeat(width));
        if i < widths.len() - 1 {
            print!("  ");
        }
    }
    println!();

    // Print data rows
    for row in &rows {
        for (i, cell) in row.iter().enumerate() {
            print!("{:<width$}", cell, width = widths[i]);
            if i < row.len() - 1 {
                print!("  ");
            }
        }
        println!();
    }

    Ok(())
}

pub fn display_prs_quiet(prs: &[PrInfo]) {
    for pr_info in prs {
        println!("{}", pr_info.pr.number);
    }
}

pub fn display_prs_verbose(prs: &[PrInfo], show_logs: bool) -> Result<()> {
    if prs.is_empty() {
        println!("No pull requests found matching filters.");
        return Ok(());
    }

    // Group PRs by repository
    let mut repos = std::collections::HashMap::new();
    for pr_info in prs {
        let repo_key = format!("{}/{}", pr_info.repo_owner, pr_info.repo_name);
        repos.entry(repo_key).or_insert_with(Vec::new).push(pr_info);
    }

    for (repo_name, repo_prs) in repos {
        println!("Repository: {}", repo_name);
        println!("=====================================");

        for pr_info in repo_prs {
            display_single_pr_tree(pr_info, show_logs)?;
        }
    }
    Ok(())
}

fn display_single_pr_tree(pr_info: &PrInfo, show_logs: bool) -> Result<()> {
    let pr = &pr_info.pr;

    // Main PR info
    println!("● {}", pr.url);
    println!("├─Title: {} ({})", pr.title, pr.author_login);
    println!("├─PR #{}", pr.number);
    println!("├─State: OPEN");
    println!("├─Created: {}", pr.created_at.format("%Y-%m-%dT%H:%M:%SZ"));

    // Status section
    println!("├─Status");
    println!(
        "│ ├─Approved: {}",
        if has_label(pr, KnownLabel::Approved) {
            "Yes"
        } else {
            "No"
        }
    );
    println!("│ ├─CI: {}", get_ci_status(&pr_info.checks)?);
    println!(
        "│ ├─LGTM: {}",
        if has_label(pr, KnownLabel::Lgtm) {
            "Yes"
        } else {
            "No"
        }
    );
    println!(
        "│ └─OK-to-test: {}",
        if has_label(pr, KnownLabel::OkToTest) {
            "Yes"
        } else {
            "No"
        }
    );

    // Labels section
    println!("├─Labels");
    if pr.labels.is_empty() {
        println!("│ └─None");
    } else {
        for (i, label) in pr.labels.iter().enumerate() {
            if i == pr.labels.len() - 1 {
                println!("│ └─{}", label);
            } else {
                println!("│ ├─{}", label);
            }
        }
    }

    // Checks section
    println!("└─Checks");
    if pr_info.checks.is_empty() {
        println!("  └─None");
    } else {
        display_checks_tree(&pr_info.checks, show_logs)?;
    }
    Ok(())
}

/// Determines the display status string for a check based on its conclusion or
/// state.
fn get_check_display_status(check: &CheckInfo) -> Result<&'static str> {
    if let Some(conclusion) = &check.conclusion {
        match conclusion {
            Conclusion::Success => Ok("SUCCESS"),
            Conclusion::Failure | Conclusion::Cancelled | Conclusion::TimedOut => Ok("FAILURE"),
            Conclusion::ActionRequired | Conclusion::Neutral | Conclusion::Skipped => Ok("PENDING"),
            unknown => anyhow::bail!("Unknown Conclusion variant in display: {:?}", unknown),
        }
    } else if let Some(state) = &check.status_state {
        match state {
            StatusState::Success => Ok("SUCCESS"),
            StatusState::Failure | StatusState::Error => Ok("FAILURE"),
            StatusState::Pending => Ok("PENDING"),
            unknown => anyhow::bail!("Unknown StatusState variant in display: {:?}", unknown),
        }
    } else {
        Ok("PENDING")
    }
}

/// Groups checks by their display status for organised tree output.
fn group_checks_by_status(
    checks: &[CheckInfo],
) -> Result<std::collections::HashMap<String, Vec<&CheckInfo>>> {
    let mut checks_by_status: std::collections::HashMap<String, Vec<&CheckInfo>> =
        std::collections::HashMap::new();
    for check in checks {
        let status = get_check_display_status(check)?;
        checks_by_status
            .entry(status.to_string())
            .or_default()
            .push(check);
    }
    Ok(checks_by_status)
}

/// Returns appropriate tree drawing prefixes based on position in the tree
/// structure.
fn get_tree_prefixes(
    is_last_group: bool,
    is_last_check: bool,
) -> (&'static str, &'static str, &'static str) {
    match (is_last_group, is_last_check) {
        (true, true) => ("    └─", "      └─", "      "),
        (true, false) => ("    ├─", "    │ └─", "    │ "),
        (false, true) => ("  │ └─", "  │   └─", "  │   "),
        (false, false) => ("  │ ├─", "  │ │ └─", "  │ │ "),
    }
}

/// Fetches and displays error logs from a failing check URL with appropriate
/// tree formatting.
fn display_check_logs(url: &str, log_prefix: &str) {
    if let Ok(logs) = fetch_and_filter_logs(url) {
        if !logs.is_empty() {
            println!("{}Error logs:", log_prefix);
            for log_line in logs {
                println!("{}{}", log_prefix, log_line);
            }
        }
    }
}

/// Displays a single check in the tree structure with optional log output.
fn display_individual_check(
    check: &CheckInfo,
    status: &str,
    is_last_group: bool,
    is_last_check: bool,
    show_logs: bool,
) {
    let (check_prefix, url_prefix, log_prefix) = get_tree_prefixes(is_last_group, is_last_check);

    println!("{}{}", check_prefix, check.name);

    if let Some(url) = &check.url {
        println!("{}URL: {}", url_prefix, url);
        if show_logs && status == "FAILURE" {
            display_check_logs(url, log_prefix);
        }
    }
}

/// Displays a group of checks with the same status in tree format.
fn display_status_group(status: &str, checks: &[&CheckInfo], is_last_group: bool, show_logs: bool) {
    let group_prefix = if is_last_group {
        "  └─"
    } else {
        "  ├─"
    };
    println!("{}{} ({})", group_prefix, status, checks.len());

    for (i, check) in checks.iter().enumerate() {
        let is_last_check = i == checks.len() - 1;
        display_individual_check(check, status, is_last_group, is_last_check, show_logs);
    }
}

/// Displays all checks in a hierarchical tree structure grouped by status.
fn display_checks_tree(checks: &[CheckInfo], show_logs: bool) -> Result<()> {
    const STATUS_ORDER: &[&str] = &["FAILURE", "PENDING", "SUCCESS", "UNKNOWN"];

    let checks_by_status = group_checks_by_status(checks)?;
    let mut displayed_groups = 0;
    let total_groups = checks_by_status.len();

    for status in STATUS_ORDER {
        if let Some(status_checks) = checks_by_status.get(*status) {
            displayed_groups += 1;
            let is_last_group = displayed_groups == total_groups;
            display_status_group(status, status_checks, is_last_group, show_logs);
        }
    }
    Ok(())
}

/// Displays pull requests using the appropriate format based on CLI mode flags.
pub fn display_prs_by_mode(prs: &[PrInfo], cli: &crate::Cli) -> Result<()> {
    if cli.quiet {
        display_prs_quiet(prs);
        Ok(())
    } else if cli.detailed || cli.detailed_with_logs {
        display_prs_verbose(prs, cli.detailed_with_logs)
    } else {
        display_prs_table(prs)
    }
}
