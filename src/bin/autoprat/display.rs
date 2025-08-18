use std::{
    collections::HashMap,
    io::{self, IsTerminal, Write},
    time::Duration,
};

use anyhow::Result;
use autoprat::{
    Action, CheckConclusion, CheckInfo, CheckName, CheckState, DisplayMode, PullRequest, Task,
};
#[cfg(test)]
use autoprat::{CheckUrl, Repo};
use chrono::{DateTime, Utc};

const LABEL_APPROVED: &str = "approved";
const LABEL_LGTM: &str = "lgtm";
const LABEL_OK_TO_TEST: &str = "ok-to-test";
const LABEL_HOLD: &str = "do-not-merge/hold";

#[derive(Debug, Clone, PartialEq)]
enum CiStatus {
    Success,
    Failure,
    Pending,
    Unknown,
}

fn get_ci_status(checks: &[CheckInfo]) -> CiStatus {
    if checks.is_empty() {
        return CiStatus::Unknown;
    }

    let mut has_failure = false;
    let mut has_pending = false;
    let mut has_success = false;

    for check in checks {
        match (&check.conclusion, &check.status_state) {
            (
                Some(
                    CheckConclusion::Failure
                    | CheckConclusion::Cancelled
                    | CheckConclusion::TimedOut,
                ),
                _,
            )
            | (_, Some(CheckState::Failure | CheckState::Error)) => {
                has_failure = true;
            }
            (Some(CheckConclusion::Success), _) | (_, Some(CheckState::Success)) => {
                has_success = true;
            }
            (None, Some(CheckState::Pending)) | (Some(CheckConclusion::ActionRequired), _) => {
                has_pending = true;
            }
            _ => {}
        }
    }

    if has_failure {
        CiStatus::Failure
    } else if has_pending {
        CiStatus::Pending
    } else if has_success {
        CiStatus::Success
    } else {
        CiStatus::Unknown
    }
}

fn format_shell_command(action: &dyn Action, pr_info: &PullRequest) -> String {
    match action.get_comment_body() {
        Some(comment_body) => {
            format!("gh pr comment {} --body \"{}\"", pr_info.url, comment_body)
        }
        None => match action.name() {
            "close" => format!("gh pr close {}", pr_info.url),
            _ => format!("# Unknown action: {}", action.name()),
        },
    }
}

fn format_relative_time(time: DateTime<Utc>) -> String {
    use chrono_humanize::HumanTime;
    HumanTime::from(time).to_string()
}

fn format_error_logs<W: Write>(
    error_lines: &[String],
    log_prefix: &str,
    writer: &mut W,
) -> Result<()> {
    if !error_lines.is_empty() {
        writeln!(writer, "{log_prefix}Error logs:")?;
        for log_line in error_lines {
            writeln!(writer, "{log_prefix}{log_line}")?;
        }
    }
    Ok(())
}

fn display_prs_by_mode<W: Write>(
    prs: &[PullRequest],
    mode: &DisplayMode,
    error_logs: Option<&HashMap<u64, HashMap<CheckName, Vec<String>>>>,
    writer: &mut W,
) -> Result<()> {
    match mode {
        DisplayMode::Quiet => display_prs_quiet(prs, writer),
        DisplayMode::Detailed => display_prs_verbose(prs, false, error_logs, writer),
        DisplayMode::DetailedWithLogs => display_prs_verbose(prs, true, error_logs, writer),
        DisplayMode::Normal => display_prs_table(prs, writer),
    }
}

fn display_prs_quiet<W: Write>(prs: &[PullRequest], writer: &mut W) -> Result<()> {
    for pr_info in prs {
        writeln!(writer, "{}", pr_info.number)?;
    }
    Ok(())
}

fn display_prs_table<W: Write>(prs: &[PullRequest], writer: &mut W) -> Result<()> {
    display_prs_table_with_width(prs, writer, None)
}

const TABLE_HEADERS: &[&str] = &[
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
const TITLE_COLUMN_INDEX: usize = TABLE_HEADERS.len() - 1;
const COLUMN_SEPARATOR: &str = "  ";
const TITLE_TRUNCATION_SUFFIX: &str = "...";
const MIN_TITLE_WIDTH_FOR_TRUNCATION: usize = 3;

fn get_terminal_width(width_override: Option<usize>) -> usize {
    if let Some(width) = width_override {
        width
    } else if io::stdout().is_terminal() {
        terminal_size::terminal_size()
            .map(|(w, _)| w.0 as usize)
            .unwrap_or(usize::MAX)
    } else {
        usize::MAX
    }
}

fn pr_to_table_row(pr: &PullRequest) -> Vec<String> {
    let ci_status = get_ci_status(&pr.checks);
    let ci_str = match ci_status {
        CiStatus::Success => "Success",
        CiStatus::Failure => "Failing",
        CiStatus::Pending => "Pending",
        CiStatus::Unknown => "Unknown",
    };

    let approved = if pr.has_label(LABEL_APPROVED) {
        "✓"
    } else {
        "✗"
    };
    let lgtm = if pr.has_label(LABEL_LGTM) {
        "✓"
    } else {
        "✗"
    };
    let ok2test = if pr.has_label(LABEL_OK_TO_TEST) {
        "✓"
    } else {
        "✗"
    };
    let hold = if pr.has_label(LABEL_HOLD) { "Y" } else { "N" };

    vec![
        pr.url.clone(),
        ci_str.to_string(),
        approved.to_string(),
        lgtm.to_string(),
        ok2test.to_string(),
        hold.to_string(),
        pr.author_simple_name.clone(),
        format_relative_time(pr.created_at),
        pr.title.clone(),
    ]
}

fn prs_to_table_rows(prs: &[PullRequest]) -> Vec<Vec<String>> {
    prs.iter().map(pr_to_table_row).collect()
}

fn calculate_column_widths(headers: &[&str], rows: &[Vec<String>]) -> Vec<usize> {
    let mut widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();

    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if i < widths.len() {
                widths[i] = widths[i].max(cell.len());
            }
        }
    }

    widths
}

fn apply_title_truncation(rows: &mut [Vec<String>], widths: &mut [usize], terminal_width: usize) {
    if terminal_width == usize::MAX {
        return;
    }

    let separator_width = COLUMN_SEPARATOR.len() * (widths.len() - 1);
    let non_title_width: usize =
        widths[..TITLE_COLUMN_INDEX].iter().sum::<usize>() + separator_width;

    if non_title_width >= terminal_width {
        return;
    }

    let available_title_width = terminal_width - non_title_width - COLUMN_SEPARATOR.len();
    let max_title_width = rows
        .iter()
        .map(|row| row.get(TITLE_COLUMN_INDEX).map_or(0, |s| s.len()))
        .max()
        .unwrap_or(0);

    if max_title_width > available_title_width
        && available_title_width > MIN_TITLE_WIDTH_FOR_TRUNCATION
    {
        widths[TITLE_COLUMN_INDEX] = available_title_width;

        for row in rows {
            if let Some(title) = row.get_mut(TITLE_COLUMN_INDEX) {
                if title.len() > available_title_width {
                    let truncate_at = available_title_width - TITLE_TRUNCATION_SUFFIX.len();
                    *title = format!("{}{}", &title[..truncate_at], TITLE_TRUNCATION_SUFFIX);
                }
            }
        }
    }
}

fn render_table_headers<W: Write>(
    headers: &[&str],
    widths: &[usize],
    writer: &mut W,
) -> Result<()> {
    for (i, header) in headers.iter().enumerate() {
        write!(writer, "{:<width$}", header, width = widths[i])?;
        if i < headers.len() - 1 {
            write!(writer, "{COLUMN_SEPARATOR}")?;
        }
    }
    writeln!(writer)?;
    Ok(())
}

fn render_table_separator<W: Write>(widths: &[usize], writer: &mut W) -> Result<()> {
    for (i, &width) in widths.iter().enumerate() {
        write!(writer, "{}", "-".repeat(width))?;
        if i < widths.len() - 1 {
            write!(writer, "{COLUMN_SEPARATOR}")?;
        }
    }
    writeln!(writer)?;
    Ok(())
}

fn render_table_rows<W: Write>(
    rows: &[Vec<String>],
    widths: &[usize],
    writer: &mut W,
) -> Result<()> {
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            write!(writer, "{:<width$}", cell, width = widths[i])?;
            if i < row.len() - 1 {
                write!(writer, "{COLUMN_SEPARATOR}")?;
            }
        }
        writeln!(writer)?;
    }
    Ok(())
}

fn display_prs_table_with_width<W: Write>(
    prs: &[PullRequest],
    writer: &mut W,
    width_override: Option<usize>,
) -> Result<()> {
    let terminal_width = get_terminal_width(width_override);
    let mut rows = prs_to_table_rows(prs);
    let mut widths = calculate_column_widths(TABLE_HEADERS, &rows);

    apply_title_truncation(&mut rows, &mut widths, terminal_width);

    render_table_headers(TABLE_HEADERS, &widths, writer)?;
    render_table_separator(&widths, writer)?;
    render_table_rows(&rows, &widths, writer)?;

    Ok(())
}

fn group_prs_by_repository(prs: &[PullRequest]) -> HashMap<String, Vec<&PullRequest>> {
    let mut repos = HashMap::new();
    for pr_info in prs {
        let repo_key = format!("{}", pr_info.repo);
        repos.entry(repo_key).or_insert_with(Vec::new).push(pr_info);
    }
    repos
}

fn display_repository_header<W: Write>(repo_name: &str, writer: &mut W) -> Result<()> {
    writeln!(writer, "Repository: {repo_name}")?;
    writeln!(writer, "=====================================")?;
    Ok(())
}

fn display_prs_verbose<W: Write>(
    prs: &[PullRequest],
    show_logs: bool,
    error_logs: Option<&HashMap<u64, HashMap<CheckName, Vec<String>>>>,
    writer: &mut W,
) -> Result<()> {
    let grouped_prs = group_prs_by_repository(prs);

    for (repo_name, repo_prs) in grouped_prs {
        display_repository_header(&repo_name, writer)?;

        for pr_info in repo_prs {
            display_single_pr_verbose(pr_info, show_logs, error_logs, writer)?;
        }
    }
    Ok(())
}

struct PrDetailFormatter<'a> {
    pr_info: &'a PullRequest,
    show_logs: bool,
    error_logs: Option<&'a HashMap<u64, HashMap<CheckName, Vec<String>>>>,
}

impl<'a> PrDetailFormatter<'a> {
    fn new(
        pr_info: &'a PullRequest,
        show_logs: bool,
        error_logs: Option<&'a HashMap<u64, HashMap<CheckName, Vec<String>>>>,
    ) -> Self {
        Self {
            pr_info,
            show_logs,
            error_logs,
        }
    }

    fn format<W: Write>(&self, writer: &mut W) -> Result<()> {
        self.write_header(writer)?;
        self.write_metadata(writer)?;
        self.write_status_section(writer)?;
        self.write_labels_section(writer)?;
        self.write_checks_section(writer)?;
        Ok(())
    }

    fn write_header<W: Write>(&self, writer: &mut W) -> Result<()> {
        writeln!(writer, "● {}", self.pr_info.url)?;
        Ok(())
    }

    fn write_metadata<W: Write>(&self, writer: &mut W) -> Result<()> {
        let pr = &self.pr_info;
        writeln!(writer, "├─Title: {} ({})", pr.title, pr.author_login)?;
        writeln!(writer, "├─PR #{}", pr.number)?;
        writeln!(writer, "├─State: OPEN")?;
        writeln!(
            writer,
            "├─Created: {}",
            pr.created_at.format("%Y-%m-%dT%H:%M:%SZ")
        )?;
        Ok(())
    }

    fn write_status_section<W: Write>(&self, writer: &mut W) -> Result<()> {
        writeln!(writer, "├─Status")?;

        let pr = &self.pr_info;
        writeln!(
            writer,
            "│ ├─Approved: {}",
            if pr.has_label(LABEL_APPROVED) {
                "Yes"
            } else {
                "No"
            }
        )?;

        let ci_status = get_ci_status(&self.pr_info.checks);
        writeln!(
            writer,
            "│ ├─CI: {}",
            match ci_status {
                CiStatus::Success => "Success",
                CiStatus::Failure => "Failing",
                CiStatus::Pending => "Pending",
                CiStatus::Unknown => "Unknown",
            }
        )?;

        writeln!(
            writer,
            "│ ├─LGTM: {}",
            if pr.has_label(LABEL_LGTM) {
                "Yes"
            } else {
                "No"
            }
        )?;

        writeln!(
            writer,
            "│ └─OK-to-test: {}",
            if pr.has_label(LABEL_OK_TO_TEST) {
                "Yes"
            } else {
                "No"
            }
        )?;

        Ok(())
    }

    fn write_labels_section<W: Write>(&self, writer: &mut W) -> Result<()> {
        writeln!(writer, "├─Labels")?;

        let labels = &self.pr_info.labels;
        if labels.is_empty() {
            writeln!(writer, "│ └─None")?;
        } else {
            for (i, label) in labels.iter().enumerate() {
                let prefix = if i == labels.len() - 1 {
                    "│ └─"
                } else {
                    "│ ├─"
                };
                writeln!(writer, "{prefix}{label}")?;
            }
        }

        Ok(())
    }

    fn write_checks_section<W: Write>(&self, writer: &mut W) -> Result<()> {
        writeln!(writer, "└─Checks")?;

        if self.pr_info.checks.is_empty() {
            writeln!(writer, "  └─None")?;
        } else {
            display_checks_tree(
                &self.pr_info.checks,
                self.show_logs,
                self.error_logs,
                self.pr_info.number,
                writer,
            )?;
        }

        Ok(())
    }
}

fn display_single_pr_verbose<W: Write>(
    pr_info: &PullRequest,
    show_logs: bool,
    error_logs: Option<&HashMap<u64, HashMap<CheckName, Vec<String>>>>,
    writer: &mut W,
) -> Result<()> {
    let formatter = PrDetailFormatter::new(pr_info, show_logs, error_logs);
    formatter.format(writer)
}

fn get_check_display_status(check: &CheckInfo) -> Result<&'static str> {
    if let Some(conclusion) = &check.conclusion {
        match conclusion {
            CheckConclusion::Success => Ok("SUCCESS"),
            CheckConclusion::Failure | CheckConclusion::Cancelled | CheckConclusion::TimedOut => {
                Ok("FAILURE")
            }
            CheckConclusion::ActionRequired
            | CheckConclusion::Neutral
            | CheckConclusion::Skipped => Ok("PENDING"),
        }
    } else if let Some(state) = &check.status_state {
        match state {
            CheckState::Success => Ok("SUCCESS"),
            CheckState::Failure | CheckState::Error => Ok("FAILURE"),
            CheckState::Pending => Ok("PENDING"),
        }
    } else {
        Ok("PENDING")
    }
}

fn group_checks_by_status(checks: &[CheckInfo]) -> Result<HashMap<String, Vec<&CheckInfo>>> {
    let mut checks_by_status: HashMap<String, Vec<&CheckInfo>> = HashMap::new();
    for check in checks {
        let status = get_check_display_status(check)?;
        checks_by_status
            .entry(status.to_string())
            .or_default()
            .push(check);
    }
    Ok(checks_by_status)
}

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

fn display_pre_fetched_error_logs<W: Write>(
    error_logs: Option<&HashMap<u64, HashMap<CheckName, Vec<String>>>>,
    pr_number: u64,
    check_name: &CheckName,
    log_prefix: &str,
    writer: &mut W,
) -> Result<()> {
    if let Some(logs) = error_logs {
        if let Some(pr_logs) = logs.get(&pr_number) {
            if let Some(error_lines) = pr_logs.get(check_name) {
                format_error_logs(error_lines, log_prefix, writer)?;
            }
        }
    }
    Ok(())
}

fn display_individual_check<W: Write>(
    check: &CheckInfo,
    is_last_group: bool,
    is_last_check: bool,
    show_logs: bool,
    error_logs: Option<&HashMap<u64, HashMap<CheckName, Vec<String>>>>,
    pr_number: u64,
    writer: &mut W,
) -> Result<()> {
    let (check_prefix, url_prefix, log_prefix) = get_tree_prefixes(is_last_group, is_last_check);

    writeln!(writer, "{}{}", check_prefix, check.name)?;

    if let Some(url) = &check.url {
        writeln!(writer, "{url_prefix}URL: {url}")?;

        if show_logs && check.is_failed() {
            display_pre_fetched_error_logs(error_logs, pr_number, &check.name, log_prefix, writer)?;
        }
    }

    Ok(())
}

fn display_status_group<W: Write>(
    status: &str,
    checks: &[&CheckInfo],
    is_last_group: bool,
    show_logs: bool,
    error_logs: Option<&HashMap<u64, HashMap<CheckName, Vec<String>>>>,
    pr_number: u64,
    writer: &mut W,
) -> Result<()> {
    let group_prefix = if is_last_group {
        "  └─"
    } else {
        "  ├─"
    };
    writeln!(writer, "{}{} ({})", group_prefix, status, checks.len())?;

    for (i, check) in checks.iter().enumerate() {
        let is_last_check = i == checks.len() - 1;
        display_individual_check(
            check,
            is_last_group,
            is_last_check,
            show_logs,
            error_logs,
            pr_number,
            writer,
        )?;
    }

    Ok(())
}

fn display_checks_tree<W: Write>(
    checks: &[CheckInfo],
    show_logs: bool,
    error_logs: Option<&HashMap<u64, HashMap<CheckName, Vec<String>>>>,
    pr_number: u64,
    writer: &mut W,
) -> Result<()> {
    const STATUS_ORDER: &[&str] = &["FAILURE", "PENDING", "SUCCESS", "UNKNOWN"];

    let checks_by_status = group_checks_by_status(checks)?;
    let mut displayed_groups = 0;
    let total_groups = checks_by_status.len();

    for status in STATUS_ORDER {
        if let Some(status_checks) = checks_by_status.get(*status) {
            displayed_groups += 1;
            let is_last_group = displayed_groups == total_groups;
            display_status_group(
                status,
                status_checks,
                is_last_group,
                show_logs,
                error_logs,
                pr_number,
                writer,
            )?;
        }
    }
    Ok(())
}

pub fn output_shell_commands<W: Write>(actions: &[Task], writer: &mut W) -> Result<()> {
    for action in actions {
        let command = format_shell_command(action.action.as_ref(), &action.pr_info);
        writeln!(writer, "{command}")?;
    }
    Ok(())
}

pub async fn display_pr_table<W: Write + Send>(
    prs: &[PullRequest],
    mode: &DisplayMode,
    writer: &mut W,
) -> Result<()> {
    use crate::log_fetcher::LogFetcher;

    let needs_logs = matches!(mode, DisplayMode::DetailedWithLogs);

    let error_logs = if needs_logs {
        const DEFAULT_CONCURRENCY: usize = 20;
        const DEFAULT_TIMEOUT_SECS: u64 = 30;

        let max_concurrent = std::env::var("AUTOPRAT_MAX_CONCURRENT_HTTP_STREAMS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_CONCURRENCY);

        let timeout_secs = std::env::var("AUTOPRAT_HTTP_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_TIMEOUT_SECS);

        let log_fetcher = LogFetcher::new(max_concurrent, Duration::from_secs(timeout_secs));
        let pr_results = log_fetcher.fetch_logs_for_prs(prs).await;

        let mut error_logs: HashMap<u64, HashMap<CheckName, Vec<String>>> = HashMap::new();
        for pr_result in &pr_results {
            for fetch_error in &pr_result.fetch_errors {
                writeln!(writer, "Warning: Failed to fetch logs for {fetch_error}")?;
            }

            if !pr_result.logs.is_empty() {
                error_logs.insert(pr_result.pr.number, pr_result.logs.clone());
            }
        }
        Some(error_logs)
    } else {
        None
    };

    display_prs_by_mode(prs, mode, error_logs.as_ref(), writer)
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;

    fn test_repo() -> Repo {
        Repo::new("owner", "repo").unwrap()
    }

    fn create_test_pr_data() -> Vec<PullRequest> {
        let base_time = Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap();

        vec![PullRequest {
            repo: test_repo(),
            number: 101,
            title: "Add authentication system".to_string(),
            author_login: "alice".to_string(),
            author_search_format: "alice".to_string(),
            author_simple_name: "alice".to_string(),
            url: "https://github.com/owner/repo/pull/101".to_string(),
            labels: vec!["enhancement".to_string(), "approved".to_string()],
            created_at: base_time - chrono::Duration::hours(5),
            checks: vec![
                CheckInfo {
                    name: CheckName::new("unit-tests").unwrap(),
                    conclusion: Some(CheckConclusion::Success),
                    status_state: None,
                    url: CheckUrl::new("https://github.com/checks/1").ok(),
                },
                CheckInfo {
                    name: CheckName::new("integration-tests").unwrap(),
                    conclusion: Some(CheckConclusion::Failure),
                    status_state: None,
                    url: CheckUrl::new("https://github.com/checks/2").ok(),
                },
            ],
            recent_comments: vec![],
        }]
    }

    fn create_display_mode(quiet: bool, detailed: bool, detailed_with_logs: bool) -> DisplayMode {
        match (quiet, detailed, detailed_with_logs) {
            (true, _, _) => DisplayMode::Quiet,
            (_, _, true) => DisplayMode::DetailedWithLogs,
            (_, true, _) => DisplayMode::Detailed,
            _ => DisplayMode::Normal,
        }
    }

    #[tokio::test]
    async fn test_display_quiet_mode() {
        let prs = create_test_pr_data();
        let mode = create_display_mode(true, false, false);
        let mut output = Vec::new();

        display_pr_table(&prs, &mode, &mut output).await.unwrap();

        let result = String::from_utf8(output).unwrap();
        assert_eq!(result, "101\n");
    }

    #[test]
    fn test_display_table_mode() {
        let prs = create_test_pr_data();
        let mut output = Vec::new();

        // Use a large fixed width in tests to prevent truncation and make tests deterministic.
        display_prs_table_with_width(&prs, &mut output, Some(usize::MAX)).unwrap();

        let result = String::from_utf8(output).unwrap();

        // Verify table structure.
        assert!(result.contains("URL"));
        assert!(result.contains("CI"));
        assert!(result.contains("APP"));
        assert!(result.contains("LGTM"));
        assert!(result.contains("OK2TST"));
        assert!(result.contains("HOLD"));
        assert!(result.contains("AUTHOR"));
        assert!(result.contains("CREATED"));
        assert!(result.contains("TITLE"));

        // Verify data row - now we can check for the full title since no truncation.
        assert!(result.contains("101"));
        assert!(result.contains("alice"));
        assert!(result.contains("Add authentication system"));
        assert!(result.contains("✗")); // CI failure.
        assert!(result.contains("✓")); // Approved.
    }

    #[tokio::test]
    async fn test_display_verbose_mode() {
        let prs = create_test_pr_data();
        let mode = create_display_mode(false, true, false);
        let mut output = Vec::new();

        display_pr_table(&prs, &mode, &mut output).await.unwrap();

        let result = String::from_utf8(output).unwrap();

        // Verify verbose structure - updated for current tree format.
        assert!(result.contains("● https://github.com/owner/repo/pull/101"));
        assert!(result.contains("├─Title: Add authentication system (alice)"));
        assert!(result.contains("├─PR #101"));
        assert!(result.contains("├─State: OPEN"));
        assert!(result.contains("├─Created:"));
        assert!(result.contains("├─Status"));
        assert!(result.contains("│ ├─Approved: Yes"));
        assert!(result.contains("│ ├─CI: Failing"));
        assert!(result.contains("├─Labels"));
        assert!(result.contains("│ ├─enhancement"));
        assert!(result.contains("│ └─approved"));
        assert!(result.contains("└─Checks"));
        assert!(result.contains("FAILURE"));
        assert!(result.contains("SUCCESS"));
        assert!(result.contains("unit-tests"));
        assert!(result.contains("integration-tests"));
    }

    #[tokio::test]
    async fn test_display_verbose_with_logs_mode() {
        let prs = create_test_pr_data();
        let mode = create_display_mode(false, false, true);
        let mut output = Vec::new();

        display_pr_table(&prs, &mode, &mut output).await.unwrap();

        let result = String::from_utf8(output).unwrap();

        // Should have same structure as verbose mode (logs feature not yet implemented).
        assert!(result.contains("● https://github.com/owner/repo/pull/101"));
        assert!(result.contains("└─Checks"));
        assert!(result.contains("FAILURE"));
        assert!(result.contains("SUCCESS"));
    }

    #[tokio::test]
    async fn test_empty_pr_list() {
        let prs = vec![];
        let mode = create_display_mode(false, false, false);
        let mut output = Vec::new();

        display_pr_table(&prs, &mode, &mut output).await.unwrap();

        let result = String::from_utf8(output).unwrap();
        // Should show headers and separator but no data rows - this is informative.
        assert!(result.contains("URL"));
        assert!(result.contains("CI"));
        assert!(result.contains("TITLE"));
        assert!(result.contains("----")); // Separator line.
        // Should not contain any PR data since list is empty.
        assert!(!result.contains("101"));
        assert!(!result.contains("alice"));
    }
}
