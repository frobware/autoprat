use std::time::Duration;

use anyhow::{Context, Result};
use clap::{Args, Parser};

use crate::{
    filters::{
        AuthorPost, BaseBranchPost, BaseBranchSearch, CommitExpr, CommitsPost, FailingCheckPost,
        FailingCiPost, LabelSearch, NeedsApproveSearch, NeedsLgtmSearch, NeedsOkToTestSearch,
        TitlePost,
    },
    pr_selector::{PrIdentifier, parse_pr_identifiers},
    search::format_user_query,
    types::{
        ActionPolicy, AppRequest, CommentAction, DisplayMode, DisplaySettings, FetchCriteria,
        PostFilter, PrAction, QuerySpec, Repo, SearchFilter, SelectionPolicy,
    },
};

const BUILD_INFO_HUMAN: &str = env!("BUILD_INFO_HUMAN");

#[derive(Args, Debug, Clone, Default)]
struct ActionArgs {
    /// Post /approve comments
    #[arg(long, help_heading = "Actions")]
    pub approve: bool,

    /// Post /lgtm comments
    #[arg(long, help_heading = "Actions")]
    pub lgtm: bool,

    /// Post /ok-to-test comments
    #[arg(long = "ok-to-test", help_heading = "Actions")]
    pub ok_to_test: bool,

    /// Post /retest comments
    #[arg(long, help_heading = "Actions")]
    pub retest: bool,

    /// Close PRs
    #[arg(long, help_heading = "Actions")]
    pub close: bool,

    /// Merge PRs
    #[arg(long, help_heading = "Actions")]
    pub merge: bool,

    /// Post /hold comments
    #[arg(long, help_heading = "Actions")]
    pub hold: bool,
}

#[derive(Args, Debug, Clone, Default)]
struct FilterArgs {
    /// Missing 'approved' label
    #[arg(long = "needs-approve", help_heading = "Filters")]
    pub needs_approve: bool,

    /// Missing 'lgtm' label
    #[arg(long = "needs-lgtm", help_heading = "Filters")]
    pub needs_lgtm: bool,

    /// Has 'needs-ok-to-test' label
    #[arg(long = "needs-ok-to-test", help_heading = "Filters")]
    pub needs_ok_to_test: bool,

    /// Has failing CI checks
    #[arg(long = "failing-ci", help_heading = "Filters")]
    pub failing_ci: bool,

    /// Exact author match
    #[arg(short = 'a', long, help_heading = "Filters", value_name = "USERNAME")]
    pub author: Option<String>,

    /// Has label (prefix - to negate, can specify multiple)
    #[arg(long, help_heading = "Filters", value_name = "NAME")]
    pub label: Vec<String>,

    /// Specific CI check is failing (exact match)
    #[arg(
        long = "failing-check",
        help_heading = "Filters",
        value_name = "CHECK-NAME"
    )]
    pub failing_check: Vec<String>,

    /// Filter by PR title (regex match)
    #[arg(short = 't', long, help_heading = "Filters", value_name = "REGEX")]
    pub title: Option<String>,

    /// Filter by base/target branch (exact match)
    #[arg(long, help_heading = "Filters", value_name = "BRANCH")]
    pub base: Option<String>,

    /// Filter by commit count: bare number (exact), or prefix with =, !=, >, >=, <, <= (e.g. '>1', '<=3')
    #[arg(long, help_heading = "Filters", value_name = "EXPR")]
    pub commits: Option<String>,
}

#[derive(Parser, Default, Debug)]
#[command(
    about = "Find and filter GitHub PRs, then optionally generate bulk action commands (approve, LGTM, retest, close, etc.)"
)]
#[command(long_version = BUILD_INFO_HUMAN)]
struct CliArgs {
    /// GitHub repository in format 'owner/repo' (can specify multiple)
    #[arg(short = 'r', long = "repo", value_name = "OWNER/REPO")]
    pub repo: Vec<String>,

    /// PR-NUMBER|PR-RANGE|PR-URL ... (a range is inclusive, e.g. 123-127)
    pub prs: Vec<String>,

    /// Exclude specific PRs from processing (can specify multiple or comma-separated)
    #[arg(
        short = 'E',
        long = "exclude",
        value_name = "PR-NUMBER|PR-RANGE|PR-URL",
        value_delimiter = ','
    )]
    pub exclude: Vec<String>,

    /// Raw GitHub search query (mutually exclusive with filter options)
    #[arg(long, value_name = "SEARCH-QUERY")]
    pub query: Option<String>,

    #[command(flatten)]
    pub actions: ActionArgs,

    #[command(flatten)]
    pub filters: FilterArgs,

    /// Post custom comment commands (can specify multiple)
    #[arg(short = 'c', long, value_name = "TEXT")]
    pub comment: Vec<String>,

    /// Skip if same comment posted recently (e.g. 5, 30s, 5m, 2h; unitless implies minutes)
    #[arg(long, value_name = "DURATION")]
    pub throttle: Option<String>,

    /// Maximum age for history check (e.g. 30m, 1h, 2h; default: 1h)
    #[arg(long, value_name = "DURATION", hide = true)]
    pub history_max_age: Option<String>,

    /// Maximum number of recent comments to check in history (default: 10)
    #[arg(long, value_name = "NUM", hide = true)]
    pub history_max_comments: Option<usize>,

    /// Show detailed PR information
    #[arg(short = 'd', long)]
    pub detailed: bool,

    /// Show detailed PR information with error logs from failing checks
    #[arg(short = 'D', long = "detailed-with-logs")]
    pub detailed_with_logs: bool,

    /// Print PR numbers only
    #[arg(short = 'q', long)]
    pub quiet: bool,

    /// Limit the number of PRs to process
    #[arg(short = 'L', long, default_value = "30", value_name = "NUM")]
    pub limit: usize,

    /// Truncate long lines to fit terminal width (like less -S)
    #[arg(short = 'S', long = "chop-long-lines")]
    pub chop_long_lines: bool,

    /// Safety guard: abort with an error (no commands emitted) when any PR targeted by an action has more than this many commits
    #[arg(long = "commit-limit", default_value = "1", value_name = "NUM")]
    pub commit_limit: u64,
}

impl CliArgs {
    pub fn validate(&self) -> Result<()> {
        if self.repo.is_empty() && self.query.is_none() && self.prs.is_empty() {
            anyhow::bail!("Must specify one of: --repo, --query, or --prs");
        }

        if self.query.is_some() {
            if !self.repo.is_empty() {
                anyhow::bail!("Cannot use --repo with --query (specify repo in query instead)");
            }
            if !self.prs.is_empty() {
                anyhow::bail!("Cannot use --prs with --query (these are different modes)");
            }
        }

        if !self.prs.is_empty() {
            if self.repo.is_empty() {
                let has_pr_numbers = self.prs.iter().any(|pr| !pr.starts_with("https://"));
                if has_pr_numbers {
                    anyhow::bail!("--repo is required when using PR numbers (not URLs)");
                }
            } else if self.repo.len() > 1 {
                anyhow::bail!(
                    "Cannot specify multiple --repo flags when using PR numbers (use PR URLs instead or specify a single repo)"
                );
            }
        }

        if !self.exclude.is_empty() {
            if self.repo.is_empty() {
                let has_pr_numbers = self.exclude.iter().any(|pr| !pr.starts_with("https://"));
                if has_pr_numbers {
                    anyhow::bail!("--repo is required when using exclude PR numbers (not URLs)");
                }
            } else if self.repo.len() > 1 {
                anyhow::bail!(
                    "Cannot specify multiple --repo flags when using exclude PR numbers (use PR URLs instead or specify a single repo)"
                );
            }
        }

        Ok(())
    }
}

fn cli_to_actions(opts: &ActionArgs, custom_comments: &[String]) -> Vec<PrAction> {
    let mut comment_actions = Vec::new();
    let mut all_actions = Vec::new();

    if opts.approve {
        comment_actions.push(CommentAction::Approve);
    }
    if opts.lgtm {
        comment_actions.push(CommentAction::Lgtm);
    }
    if opts.ok_to_test {
        comment_actions.push(CommentAction::OkToTest);
    }
    if opts.retest {
        comment_actions.push(CommentAction::Retest);
    }
    if opts.hold {
        comment_actions.push(CommentAction::Hold);
    }

    for comment in custom_comments {
        comment_actions.push(CommentAction::Custom(comment.clone()));
    }

    if let Some(action) = PrAction::comments(comment_actions) {
        all_actions.push(action);
    }

    if opts.close {
        all_actions.push(PrAction::Close);
    }
    if opts.merge {
        all_actions.push(PrAction::Merge);
    }

    all_actions
}

fn cli_to_search_filters(filter_args: &FilterArgs) -> Vec<Box<dyn SearchFilter + Send + Sync>> {
    let mut out: Vec<Box<dyn SearchFilter + Send + Sync>> = Vec::new();
    if filter_args.needs_approve {
        out.push(Box::new(NeedsApproveSearch));
    }
    if filter_args.needs_lgtm {
        out.push(Box::new(NeedsLgtmSearch));
    }
    if filter_args.needs_ok_to_test {
        out.push(Box::new(NeedsOkToTestSearch));
    }

    if !filter_args.label.is_empty() {
        out.push(Box::new(LabelSearch {
            labels: filter_args.label.clone(),
        }));
    }

    if let Some(branch) = &filter_args.base {
        out.push(Box::new(BaseBranchSearch {
            branch: branch.clone(),
        }));
    }

    out
}

fn cli_to_post_filters(filter_args: &FilterArgs) -> Result<Vec<Box<dyn PostFilter + Send + Sync>>> {
    let mut out: Vec<Box<dyn PostFilter + Send + Sync>> = Vec::new();
    if filter_args.failing_ci {
        out.push(Box::new(FailingCiPost));
    }
    if let Some(name) = &filter_args.author {
        out.push(Box::new(AuthorPost::new().with_value(name.clone())));
    }
    if !filter_args.failing_check.is_empty() {
        out.push(Box::new(FailingCheckPost {
            check_names: filter_args.failing_check.clone(),
        }));
    }
    if let Some(title) = &filter_args.title {
        out.push(Box::new(TitlePost::new().with_value(title.clone())));
    }

    if let Some(branch) = &filter_args.base {
        out.push(Box::new(BaseBranchPost::new().with_value(branch.clone())));
    }

    if let Some(expr) = &filter_args.commits {
        out.push(Box::new(CommitsPost {
            expr: CommitExpr::parse(expr)?,
        }));
    }

    Ok(out)
}

fn parse_throttle_duration(throttle_str: &str) -> Result<Duration> {
    let throttle_str = throttle_str.trim();

    if let Ok(minutes) = throttle_str.parse::<u64>() {
        return Ok(Duration::from_secs(minutes * 60));
    }

    if let Some(seconds_str) = throttle_str.strip_suffix('s') {
        let seconds: u64 = seconds_str
            .parse()
            .with_context(|| format!("Invalid throttle seconds: '{seconds_str}'"))?;
        return Ok(Duration::from_secs(seconds));
    }

    if let Some(minutes_str) = throttle_str.strip_suffix('m') {
        let minutes: u64 = minutes_str
            .parse()
            .with_context(|| format!("Invalid throttle minutes: '{minutes_str}'"))?;
        return Ok(Duration::from_secs(minutes * 60));
    }

    if let Some(hours_str) = throttle_str.strip_suffix('h') {
        let hours: u64 = hours_str
            .parse()
            .with_context(|| format!("Invalid throttle hours: '{hours_str}'"))?;
        return Ok(Duration::from_secs(hours * 3600));
    }

    anyhow::bail!(
        "Invalid throttle format '{}'. Supported formats: unitless number (minutes), '30s', '5m', '2h'",
        throttle_str
    )
}

fn validate_pr_urls_against_repo(repos: &[String], prs: &[String]) -> Result<()> {
    // Only validate if there's exactly one repo specified
    if repos.len() != 1 {
        return Ok(());
    }

    let expected_repo = Repo::parse(&repos[0])
        .map_err(|e| anyhow::anyhow!("Invalid repository format '{}': {}", repos[0], e))?;

    for pr in prs {
        if pr.starts_with("https://") {
            let pr_repo = Repo::parse_url(pr)?;
            if pr_repo != expected_repo {
                anyhow::bail!(
                    "PR URL {} is from {} but --repo specifies {}",
                    pr,
                    pr_repo,
                    expected_repo
                );
            }
        }
    }

    Ok(())
}

fn parse_pr_args_to_identifiers(repos: &[Repo], prs: &[String]) -> Result<Vec<PrIdentifier>> {
    parse_pr_identifiers(repos.first(), prs).map_err(anyhow::Error::new)
}

fn determine_display_settings(cli: &CliArgs) -> DisplaySettings {
    let mode = match (cli.quiet, cli.detailed, cli.detailed_with_logs) {
        (true, _, _) => DisplayMode::Quiet,
        (_, _, true) => DisplayMode::DetailedWithLogs,
        (_, true, _) => DisplayMode::Detailed,
        _ => DisplayMode::Normal,
    };

    DisplaySettings {
        mode,
        truncate_titles: cli.chop_long_lines,
    }
}

fn create_autoprat_request(cli: CliArgs) -> Result<QuerySpec> {
    cli.validate()?;

    let repos: Result<Vec<Repo>> = cli
        .repo
        .iter()
        .map(|r| {
            Repo::parse(r).map_err(|e| anyhow::anyhow!("Invalid repository format '{}': {}", r, e))
        })
        .collect();
    let repos = repos?;

    validate_pr_urls_against_repo(&cli.repo, &cli.prs)?;
    validate_pr_urls_against_repo(&cli.repo, &cli.exclude)?;
    let pr_identifiers = parse_pr_args_to_identifiers(&repos, &cli.prs)?;
    let exclude_identifiers = parse_pr_args_to_identifiers(&repos, &cli.exclude)?;

    let query = cli.query.as_ref().map(|q| format_user_query(q));

    let throttle = cli
        .throttle
        .as_ref()
        .filter(|t| !t.trim().is_empty())
        .map(|t| parse_throttle_duration(t))
        .transpose()?;

    let history_max_age = cli
        .history_max_age
        .as_ref()
        .filter(|t| !t.trim().is_empty())
        .map(|t| parse_throttle_duration(t))
        .transpose()?
        .unwrap_or(Duration::from_secs(60 * 60)); // Default: 1 hour

    let history_max_comments = cli.history_max_comments.unwrap_or(10); // Default: 10

    Ok(QuerySpec {
        fetch: FetchCriteria {
            repos,
            prs: pr_identifiers,
            query,
            limit: cli.limit,
            search_filters: cli_to_search_filters(&cli.filters),
        },
        selection: SelectionPolicy {
            exclude: exclude_identifiers,
            post_filters: cli_to_post_filters(&cli.filters)?,
        },
        action_policy: ActionPolicy {
            actions: cli_to_actions(&cli.actions, &cli.comment),
            throttle,
            history_max_age,
            history_max_comments,
            commit_limit: cli.commit_limit,
        },
    })
}

fn transform_slash_commands(args: Vec<String>) -> Vec<String> {
    args.into_iter()
        .map(|arg| match arg.as_str() {
            "/approve" => "--approve".to_string(),
            "/lgtm" => "--lgtm".to_string(),
            "/ok-to-test" => "--ok-to-test".to_string(),
            "/retest" => "--retest".to_string(),
            "/close" => "--close".to_string(),
            "/merge" => "--merge".to_string(),
            "/hold" => "--hold".to_string(),
            _ => arg,
        })
        .collect()
}

fn build_query_from_cli(cli: CliArgs) -> Result<AppRequest> {
    let display = determine_display_settings(&cli);
    let query = create_autoprat_request(cli)?;
    Ok(AppRequest { query, display })
}

/// Parses command-line arguments into a query specification and display mode.
///
/// Transforms slash commands (e.g., /retest) into standard arguments and
/// validates all inputs according to CLI rules. Returns structured query
/// parameters ready for execution.
pub fn parse_args<I, T>(args: I) -> Result<AppRequest>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let args_vec: Vec<String> = args
        .into_iter()
        .map(|arg| arg.into().into_string().unwrap())
        .collect();
    let transformed_args = transform_slash_commands(args_vec);

    let cli = CliArgs::try_parse_from(transformed_args)?;
    build_query_from_cli(cli)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repos() -> Vec<Repo> {
        vec![Repo::new("openshift", "bpfman-operator").unwrap()]
    }

    #[test]
    fn parse_pr_args_to_identifiers_rejects_bare_dash_with_helpful_message() {
        let err = parse_pr_args_to_identifiers(&repos(), &["-".to_string()])
            .expect_err("expected bare '-' to be rejected");
        let rendered = format!("{err:#}");

        assert!(
            rendered.contains('-'),
            "error should quote the offending identifier, got: {rendered}"
        );
        assert!(
            rendered.contains("PR number") || rendered.contains("URL"),
            "error should describe expected format, got: {rendered}"
        );
        assert!(
            !rendered.contains("invalid digit"),
            "error should not leak std parser detail, got: {rendered}"
        );
    }

    #[test]
    fn parse_pr_args_to_identifiers_rejects_non_numeric_with_helpful_message() {
        let err = parse_pr_args_to_identifiers(&repos(), &["red-hat-konflux".to_string()])
            .expect_err("expected non-numeric identifier to be rejected");
        let rendered = format!("{err:#}");

        assert!(
            !rendered.contains("invalid digit"),
            "error should not leak std parser detail, got: {rendered}"
        );
        assert!(
            rendered.contains("PR number") || rendered.contains("URL"),
            "error should describe expected format, got: {rendered}"
        );
    }

    #[test]
    fn parse_pr_args_to_identifiers_accepts_numeric_pr() {
        let identifiers = parse_pr_args_to_identifiers(&repos(), &["123".to_string()])
            .expect("numeric PR should parse");
        assert_eq!(identifiers.len(), 1);
        assert_eq!(identifiers[0].number, 123);
    }

    #[test]
    fn parse_pr_args_to_identifiers_expands_inclusive_range() {
        let identifiers = parse_pr_args_to_identifiers(&repos(), &["1967-1969".to_string()])
            .expect("range should expand");
        let numbers: Vec<u64> = identifiers.iter().map(|id| id.number).collect();
        assert_eq!(numbers, vec![1967, 1968, 1969]);
    }

    #[test]
    fn parse_pr_args_to_identifiers_expands_multiple_ranges_and_singletons() {
        let identifiers = parse_pr_args_to_identifiers(
            &repos(),
            &["1-3".to_string(), "9".to_string(), "11-12".to_string()],
        )
        .expect("mixed ranges and singletons should expand in order");
        let numbers: Vec<u64> = identifiers.iter().map(|id| id.number).collect();
        assert_eq!(numbers, vec![1, 2, 3, 9, 11, 12]);
    }

    #[test]
    fn parse_pr_args_to_identifiers_treats_equal_bounds_as_single_pr() {
        let identifiers = parse_pr_args_to_identifiers(&repos(), &["42-42".to_string()])
            .expect("degenerate range should yield one PR");
        let numbers: Vec<u64> = identifiers.iter().map(|id| id.number).collect();
        assert_eq!(numbers, vec![42]);
    }

    #[test]
    fn parse_pr_args_to_identifiers_rejects_reversed_range() {
        let err = parse_pr_args_to_identifiers(&repos(), &["1969-1967".to_string()])
            .expect_err("reversed range should be rejected");
        let rendered = format!("{err:#}");
        assert!(
            rendered.contains("1969-1967"),
            "error should quote the offending range, got: {rendered}"
        );
        assert!(
            rendered.contains("1969") && rendered.contains("1967"),
            "error should name both bounds, got: {rendered}"
        );
    }
}
