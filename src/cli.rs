use std::time::Duration;

use anyhow::{Context, Result};
use clap::{Args, Parser};

use crate::{
    filters::{
        AuthorPost, BaseBranchPost, CommitExpr, CommitsPost, FailingCheckPost, FailingCiPost,
        TitlePost,
    },
    pr_selector::{PrIdentifier, parse_pr_identifiers},
    types::{
        ActionPolicy, AppRequest, CommentAction, DisplayMode, DisplaySettings, FetchCriteria,
        PostFilter, PrAction, QuerySpec, Repo, SearchCriterion, SelectionPolicy,
    },
};

const BUILD_INFO_HUMAN: &str = env!("BUILD_INFO_HUMAN");

const THROTTLE_FORMAT_HELP: &str =
    "a duration such as `30s`, `5m`, or `2h`; a bare number is read as minutes";

const DEFAULT_HISTORY_MAX_AGE: Duration = Duration::from_secs(60 * 60);
const DEFAULT_HISTORY_MAX_COMMENTS: usize = 10;

#[derive(Args, Debug, Clone, Default)]
struct ActionArgs {
    /// Emit an `/approve` comment command for each selected PR.
    #[arg(long, help_heading = "Actions")]
    pub approve: bool,

    /// Emit a `/lgtm` comment command for each selected PR.
    #[arg(long, help_heading = "Actions")]
    pub lgtm: bool,

    /// Emit an `/ok-to-test` comment command for each selected PR.
    #[arg(long = "ok-to-test", help_heading = "Actions")]
    pub ok_to_test: bool,

    /// Emit a `/retest` comment command for each selected PR.
    #[arg(long, help_heading = "Actions")]
    pub retest: bool,

    /// Emit a `gh pr close` command for each selected PR.
    #[arg(long, help_heading = "Actions")]
    pub close: bool,

    /// Emit a `gh pr merge` command for each selected PR.
    ///
    /// Draft PRs are skipped, since GitHub will not merge a draft.
    #[arg(long, help_heading = "Actions")]
    pub merge: bool,

    /// Emit a `/hold` comment command for each selected PR.
    #[arg(long, help_heading = "Actions")]
    pub hold: bool,
}

#[derive(Args, Debug, Clone, Default)]
struct FilterArgs {
    /// Keep only PRs missing the `approved` label.
    #[arg(long = "needs-approve", help_heading = "Filters")]
    pub needs_approve: bool,

    /// Keep only PRs missing the `lgtm` label.
    #[arg(long = "needs-lgtm", help_heading = "Filters")]
    pub needs_lgtm: bool,

    /// Keep only PRs carrying the `needs-ok-to-test` label.
    #[arg(long = "needs-ok-to-test", help_heading = "Filters")]
    pub needs_ok_to_test: bool,

    /// Keep only PRs with at least one failing CI check.
    #[arg(long = "failing-ci", help_heading = "Filters")]
    pub failing_ci: bool,

    /// Keep only PRs opened by this user (exact login match).
    #[arg(short = 'a', long, help_heading = "Filters", value_name = "USERNAME")]
    pub author: Option<String>,

    /// Keep only PRs with this label; repeatable.
    ///
    /// Prefix a name with `-` to require its absence instead, e.g.
    /// `--label bug --label -wip`.
    #[arg(long, help_heading = "Filters", value_name = "NAME")]
    pub label: Vec<String>,

    /// Keep only PRs where the named CI check is failing (exact name).
    ///
    /// Repeatable; a PR matches only when every named check is failing.
    #[arg(
        long = "failing-check",
        help_heading = "Filters",
        value_name = "CHECK-NAME"
    )]
    pub failing_check: Vec<String>,

    /// Keep only PRs whose title matches this regular expression.
    #[arg(short = 't', long, help_heading = "Filters", value_name = "REGEX")]
    pub title: Option<String>,

    /// Keep only PRs targeting this base branch (exact match).
    #[arg(long, help_heading = "Filters", value_name = "BRANCH")]
    pub base: Option<String>,

    /// Keep only PRs whose commit count matches this expression.
    ///
    /// A bare number matches exactly; prefix with `=`, `!=`, `>`, `>=`,
    /// `<`, or `<=` to compare, e.g. `--commits '>1'` or
    /// `--commits '<=3'`.
    #[arg(long, help_heading = "Filters", value_name = "EXPR")]
    pub commits: Option<String>,
}

#[derive(Parser, Default, Debug)]
#[command(
    about = "Find and filter GitHub PRs, then optionally generate bulk action commands (approve, LGTM, retest, close, etc.)",
    long_about = "Find and filter GitHub PRs, then optionally generate bulk action commands (approve, LGTM, retest, close, etc.).\n\nActions are not run for you: each action flag prints a `gh` command on stdout, so you can review the batch and pipe it to a shell.",
    max_term_width = 80
)]
#[command(long_version = BUILD_INFO_HUMAN)]
struct CliArgs {
    /// GitHub repository to search, as `owner/repo`.
    ///
    /// Repeat the flag to search several repositories in one run.
    #[arg(short = 'r', long = "repo", value_name = "OWNER/REPO")]
    pub repo: Vec<String>,

    /// Pull requests to act on: a number, an inclusive range, or a URL.
    ///
    /// Numbers and ranges like `123` or `123-127` need `--repo` to name
    /// the repository; full PR URLs carry their own, so they can be
    /// mixed across repositories.
    pub prs: Vec<String>,

    /// Drop these PRs from the selection.
    ///
    /// Takes the same number, range, or URL forms as the positional
    /// arguments, repeated or comma-separated, e.g. `-E 124,130-132`.
    #[arg(
        short = 'E',
        long = "exclude",
        value_name = "PR-NUMBER|PR-RANGE|PR-URL",
        value_delimiter = ','
    )]
    pub exclude: Vec<String>,

    /// Raw GitHub search query, used in place of `--repo`.
    ///
    /// Cannot be combined with `--repo`, positional PR numbers, or the
    /// label filters (`--label`, `--needs-approve`, `--needs-lgtm`,
    /// `--needs-ok-to-test`); express those as query terms instead,
    /// e.g. `repo:owner/name label:bug`. The other filter options
    /// apply as normal. `is:pr` and `is:open` are added unless you
    /// supply them; pass `is:closed` to include closed and merged PRs.
    #[arg(long, value_name = "SEARCH-QUERY")]
    pub query: Option<String>,

    #[command(flatten)]
    pub actions: ActionArgs,

    #[command(flatten)]
    pub filters: FilterArgs,

    /// Post a custom comment on each selected PR; repeatable.
    ///
    /// The text is emitted as a `gh pr comment` command and is subject
    /// to the same throttling and history checks as the action flags.
    #[arg(short = 'c', long, value_name = "TEXT")]
    pub comment: Vec<String>,

    /// Skip a comment when the same one was posted within this window.
    #[arg(
        long,
        value_name = "DURATION",
        long_help = format!(
            "Skip a comment when the same one was posted within this window.\n\nAccepts {THROTTLE_FORMAT_HELP}. Affects comment actions only."
        )
    )]
    pub throttle: Option<String>,

    /// Maximum age for history check (e.g. 30m, 1h, 2h); defaults to
    /// `DEFAULT_HISTORY_MAX_AGE`.
    #[arg(long, value_name = "DURATION", hide = true)]
    pub history_max_age: Option<String>,

    /// Maximum number of recent comments to check in history; defaults
    /// to `DEFAULT_HISTORY_MAX_COMMENTS`.
    #[arg(long, value_name = "NUM", hide = true)]
    pub history_max_comments: Option<usize>,

    /// Show each PR in detail instead of the one-line table.
    ///
    /// Expands the state, labels, and per-check CI results beneath
    /// every selected PR.
    #[arg(short = 'd', long)]
    pub detailed: bool,

    /// Like `--detailed`, but also include error logs from failing
    /// checks.
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

    /// Refuse to act when a target PR has more than this many commits.
    ///
    /// A guard against runaway bulk actions: if any PR an action would
    /// touch has more commits than this, autoprat emits no commands at
    /// all. Raise it to allow multi-commit PRs.
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
            if self.filters.needs_approve
                || self.filters.needs_lgtm
                || self.filters.needs_ok_to_test
                || !self.filters.label.is_empty()
            {
                anyhow::bail!(
                    "Cannot use label filters (--label, --needs-approve, --needs-lgtm, --needs-ok-to-test) with --query (put label terms in the query instead)"
                );
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

fn cli_to_search_criteria(filter_args: &FilterArgs) -> Vec<SearchCriterion> {
    let mut out = Vec::new();
    if filter_args.needs_approve {
        out.push(SearchCriterion::MissingLabel("approved".to_string()));
    }
    if filter_args.needs_lgtm {
        out.push(SearchCriterion::MissingLabel("lgtm".to_string()));
    }
    if filter_args.needs_ok_to_test {
        out.push(SearchCriterion::PresentLabel(
            "needs-ok-to-test".to_string(),
        ));
    }

    for label in &filter_args.label {
        if let Some(label) = label.strip_prefix('-') {
            out.push(SearchCriterion::MissingLabel(label.to_string()));
        } else {
            out.push(SearchCriterion::PresentLabel(label.clone()));
        }
    }

    if let Some(branch) = &filter_args.base {
        out.push(SearchCriterion::BaseBranch(branch.clone()));
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

    anyhow::bail!("Invalid throttle format '{throttle_str}'. Accepts {THROTTLE_FORMAT_HELP}.")
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

    let query = cli.query.clone();

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
        .unwrap_or(DEFAULT_HISTORY_MAX_AGE);

    let history_max_comments = cli
        .history_max_comments
        .unwrap_or(DEFAULT_HISTORY_MAX_COMMENTS);

    Ok(QuerySpec {
        fetch: FetchCriteria {
            repos,
            prs: pr_identifiers,
            query,
            limit: cli.limit,
            search_criteria: cli_to_search_criteria(&cli.filters),
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
    use clap::CommandFactory;

    use super::*;

    fn repos() -> Vec<Repo> {
        vec![Repo::new("openshift", "bpfman-operator").unwrap()]
    }

    #[test]
    fn help_shows_public_flags_and_hides_internal_history_knobs() {
        let help = CliArgs::command().render_long_help().to_string();

        assert!(help.contains("--approve"));
        assert!(help.contains("--ok-to-test"));
        assert!(help.contains("--needs-approve"));
        assert!(help.contains("--commits"));
        assert!(help.contains("--exclude"));
        assert!(!help.contains("--history-max-age"));
        assert!(!help.contains("--history-max-comments"));
    }

    #[test]
    fn parse_args_rejects_label_filters_with_query() {
        for args in [
            vec!["autoprat", "--query", "repo:o/r", "--label", "bug"],
            vec!["autoprat", "--query", "repo:o/r", "--needs-approve"],
            vec!["autoprat", "--query", "repo:o/r", "--needs-lgtm"],
            vec!["autoprat", "--query", "repo:o/r", "--needs-ok-to-test"],
        ] {
            let err =
                parse_args(args.clone()).expect_err(&format!("expected {args:?} to be rejected"));
            let rendered = format!("{err:#}");
            assert!(
                rendered.contains("--query"),
                "error should mention --query, got: {rendered}"
            );
        }
    }

    #[test]
    fn parse_args_allows_post_filters_with_query() {
        parse_args(["autoprat", "--query", "repo:o/r", "--author", "alice"])
            .expect("post filters should combine with --query");
    }

    #[test]
    fn parse_args_maps_action_flags_to_action_policy() {
        let request = parse_args([
            "autoprat",
            "--repo",
            "owner/repo",
            "--approve",
            "--lgtm",
            "--comment",
            "Please review",
            "--close",
            "--merge",
            "--throttle",
            "5m",
            "--history-max-age",
            "30m",
            "--history-max-comments",
            "2",
            "--commit-limit",
            "3",
        ])
        .unwrap();

        assert_eq!(
            request.query.action_policy.actions,
            vec![
                PrAction::GroupedComment(vec![
                    CommentAction::Approve,
                    CommentAction::Lgtm,
                    CommentAction::Custom("Please review".to_string()),
                ]),
                PrAction::Close,
                PrAction::Merge,
            ]
        );
        assert_eq!(
            request.query.action_policy.throttle,
            Some(Duration::from_secs(5 * 60))
        );
        assert_eq!(
            request.query.action_policy.history_max_age,
            Duration::from_secs(30 * 60)
        );
        assert_eq!(request.query.action_policy.history_max_comments, 2);
        assert_eq!(request.query.action_policy.commit_limit, 3);
    }

    #[test]
    fn parse_args_maps_exclude_ranges_to_selection_policy() {
        let request = parse_args([
            "autoprat",
            "--repo",
            "owner/repo",
            "--exclude",
            "1-3,5",
            "10",
        ])
        .unwrap();

        assert_eq!(
            request.query.selection.exclude,
            vec![
                PrIdentifier::new(Repo::new("owner", "repo").unwrap(), 1),
                PrIdentifier::new(Repo::new("owner", "repo").unwrap(), 2),
                PrIdentifier::new(Repo::new("owner", "repo").unwrap(), 3),
                PrIdentifier::new(Repo::new("owner", "repo").unwrap(), 5),
            ]
        );
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
