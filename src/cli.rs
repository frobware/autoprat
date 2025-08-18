use std::time::Duration;

use anyhow::{Context, Result};
use clap::{Args, Parser};

use crate::types::{Action, DisplayMode, PostFilter, PullRequest, QuerySpec, Repo, SearchFilter};

const BUILD_INFO_HUMAN: &str = env!("BUILD_INFO_HUMAN");

macro_rules! define_action {
    ($vis:vis $ty:ident, $name:expr, $only_if:expr, $comment:expr) => {
        #[derive(Debug, Clone)]
        $vis struct $ty;

        impl Action for $ty {
            fn name(&self) -> &'static str {
                $name
            }

            fn only_if(&self, pr_info: &PullRequest) -> bool {
                $only_if(pr_info)
            }

            fn get_comment_body(&self) -> Option<&'static str> {
                $comment
            }

            fn clone_box(&self) -> Box<dyn Action + Send + Sync> {
                Box::new(self.clone())
            }
        }
    };
}

macro_rules! simple_search_filter {
    ($vis:vis $ty:ident, $apply:expr, $matches:expr) => {
        #[derive(Debug)]
        $vis struct $ty;

        impl SearchFilter for $ty {
            fn apply(&self, terms: &mut Vec<String>) {
                ($apply)(terms)
            }

            fn matches(&self, pr: &PullRequest) -> bool {
                ($matches)(pr)
            }
        }
    };
}

macro_rules! multi_search_filter {
    ($vis:vis $ty:ident, $field:ident, $apply:expr, $matches:expr) => {
        #[derive(Debug, Clone)]
        $vis struct $ty {
            pub $field: Vec<String>,
        }
        impl SearchFilter for $ty {
            fn apply(&self, terms: &mut Vec<String>) {
                ($apply)(&self.$field, terms)
            }
            fn matches(&self, pr: &PullRequest) -> bool {
                ($matches)(&self.$field, pr)
            }
        }
    };
}

macro_rules! simple_post_filter {
    ($vis:vis $ty:ident, $pred:expr) => {
        #[derive(Debug)]
        $vis struct $ty;
        impl PostFilter for $ty {
            fn matches(&self, pr: &PullRequest) -> bool {
                ($pred)(pr)
            }
        }
    };
}

macro_rules! single_post_filter {
    ($vis:vis $ty:ident, $field:ident, $pred:expr) => {
        #[derive(Debug, Clone)]
        $vis struct $ty {
            $field: Option<String>,
        }

        impl $ty {
            pub const fn new() -> Self {
                Self { $field: None }
            }
            pub fn with_value(mut self, v: impl Into<String>) -> Self {
                self.$field = Some(v.into());
                self
            }
        }

        impl PostFilter for $ty {
            fn matches(&self, pr: &PullRequest) -> bool {
                match &self.$field {
                    Some(val) => ($pred)(pr, val),
                    None => true,
                }
            }
        }
    };
}

macro_rules! multi_post_filter {
    ($vis:vis $ty:ident, $field:ident, $pred:expr) => {
        #[derive(Debug, Clone)]
        $vis struct $ty {
            pub $field: Vec<String>,
        }


        impl PostFilter for $ty {
            fn matches(&self, pr: &PullRequest) -> bool {
                if self.$field.is_empty() {
                    return true;
                }
                ($pred)(&self.$field, pr)
            }
        }
    };
}

define_action!(
    Approve,
    "approve",
    |pr: &PullRequest| !pr.has_label("approved"),
    Some("/approve")
);

define_action!(
    Lgtm,
    "lgtm",
    |pr: &PullRequest| !pr.has_label("lgtm"),
    Some("/lgtm")
);

define_action!(
    OkToTest,
    "ok-to-test",
    |pr: &PullRequest| pr.has_label("needs-ok-to-test"),
    Some("/ok-to-test")
);

define_action!(Retest, "retest", |_| true, Some("/retest"));

define_action!(Close, "close", |_| true, None);

/// Action that posts a custom comment on a pull request.
///
/// Allows arbitrary comment text to be posted, with optional throttling
/// to prevent duplicate comments within a specified time window.
#[derive(Debug, Clone)]
pub struct CommentAction {
    pub comment: String,
}

impl CommentAction {
    pub fn new(comment: impl Into<String>) -> Self {
        Self {
            comment: comment.into(),
        }
    }
}

impl Action for CommentAction {
    fn name(&self) -> &'static str {
        "custom-comment"
    }
    fn only_if(&self, _pr_info: &PullRequest) -> bool {
        true
    }
    fn get_comment_body(&self) -> Option<&str> {
        Some(&self.comment)
    }
    fn clone_box(&self) -> Box<dyn Action + Send + Sync> {
        Box::new(self.clone())
    }
}

simple_search_filter!(
    NeedsApproveSF,
    |terms: &mut Vec<String>| terms.push("-label:approved".into()),
    |pr: &PullRequest| !pr.has_label("approved")
);

simple_search_filter!(
    NeedsLgtmSF,
    |terms: &mut Vec<String>| terms.push("-label:lgtm".into()),
    |pr: &PullRequest| !pr.has_label("lgtm")
);

simple_search_filter!(
    NeedsOkToTestSF,
    |terms: &mut Vec<String>| terms.push("label:needs-ok-to-test".into()),
    |pr: &PullRequest| pr.has_label("needs-ok-to-test")
);

multi_search_filter!(
    LabelSF,
    labels,
    |names: &[String], terms: &mut Vec<String>| {
        for lbl in names {
            if let Some(neg) = lbl.strip_prefix('-') {
                terms.push(format!("-label:{}", neg));
            } else {
                terms.push(format!("label:{}", lbl));
            }
        }
    },
    |names: &[String], pr: &PullRequest| {
        for lbl in names {
            if let Some(neg) = lbl.strip_prefix('-') {
                if pr.has_label(neg) {
                    return false;
                }
            } else if !pr.has_label(lbl) {
                return false;
            }
        }
        true
    }
);

simple_post_filter!(FailingCiPF, |pr: &PullRequest| { pr.has_failing_ci() });

single_post_filter!(AuthorPF, author, |pr: &PullRequest, name: &str| {
    pr.matches_author(name)
});

multi_post_filter!(
    FailingCheckPF,
    check_names,
    |names: &[String], pr: &PullRequest| { names.iter().all(|n| pr.has_failing_check(n)) }
);

single_post_filter!(TitlePF, title, |pr: &PullRequest, title: &str| {
    pr.title.contains(title)
});

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

    /// Filter by PR title (case-sensitive substring match)
    #[arg(short = 't', long, help_heading = "Filters", value_name = "TITLE")]
    pub title: Option<String>,
}

#[derive(Parser, Default, Debug)]
#[command(
    about = "Find and filter GitHub PRs, then optionally generate bulk action commands (approve, LGTM, retest, close, etc.)"
)]
#[command(long_version = BUILD_INFO_HUMAN)]
struct CliArgs {
    /// GitHub repository in format 'owner/repo' (required when using numeric PR arguments or no PR arguments)
    #[arg(short = 'r', long = "repo", value_name = "OWNER/REPO")]
    pub repo: Option<String>,

    /// PR-NUMBER|PR-URL ...
    pub prs: Vec<String>,

    /// Exclude specific PRs from processing (can specify multiple or comma-separated)
    #[arg(
        short = 'E',
        long = "exclude",
        value_name = "PR-NUMBER|PR-URL",
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
}

impl CliArgs {
    pub fn validate(&self) -> Result<()> {
        if self.repo.is_none() && self.query.is_none() && self.prs.is_empty() {
            anyhow::bail!("Must specify one of: --repo, --query, or --prs");
        }

        if self.query.is_some() {
            if self.repo.is_some() {
                anyhow::bail!("Cannot use --repo with --query (specify repo in query instead)");
            }
            if !self.prs.is_empty() {
                anyhow::bail!("Cannot use --prs with --query (these are different modes)");
            }
        }

        if !self.prs.is_empty() && self.repo.is_none() {
            let has_pr_numbers = self.prs.iter().any(|pr| !pr.starts_with("https://"));
            if has_pr_numbers {
                anyhow::bail!("--repo is required when using PR numbers (not URLs)");
            }
        }

        if !self.exclude.is_empty() && self.repo.is_none() {
            let has_pr_numbers = self.exclude.iter().any(|pr| !pr.starts_with("https://"));
            if has_pr_numbers {
                anyhow::bail!("--repo is required when using exclude PR numbers (not URLs)");
            }
        }

        Ok(())
    }
}

fn cli_to_actions(opts: &ActionArgs) -> Vec<Box<dyn Action + Send + Sync>> {
    let mut out: Vec<Box<dyn Action + Send + Sync>> = Vec::new();
    if opts.approve {
        out.push(Box::new(Approve));
    }
    if opts.lgtm {
        out.push(Box::new(Lgtm));
    }
    if opts.ok_to_test {
        out.push(Box::new(OkToTest));
    }
    if opts.retest {
        out.push(Box::new(Retest));
    }
    if opts.close {
        out.push(Box::new(Close));
    }
    out
}

fn cli_to_search_filters(filter_args: &FilterArgs) -> Vec<Box<dyn SearchFilter + Send + Sync>> {
    let mut out: Vec<Box<dyn SearchFilter + Send + Sync>> = Vec::new();
    if filter_args.needs_approve {
        out.push(Box::new(NeedsApproveSF));
    }
    if filter_args.needs_lgtm {
        out.push(Box::new(NeedsLgtmSF));
    }
    if filter_args.needs_ok_to_test {
        out.push(Box::new(NeedsOkToTestSF));
    }

    if !filter_args.label.is_empty() {
        out.push(Box::new(LabelSF {
            labels: filter_args.label.clone(),
        }));
    }

    out
}

fn cli_to_post_filters(filter_args: &FilterArgs) -> Vec<Box<dyn PostFilter + Send + Sync>> {
    let mut out: Vec<Box<dyn PostFilter + Send + Sync>> = Vec::new();
    if filter_args.failing_ci {
        out.push(Box::new(FailingCiPF));
    }
    if let Some(name) = &filter_args.author {
        out.push(Box::new(AuthorPF::new().with_value(name.clone())));
    }
    if !filter_args.failing_check.is_empty() {
        out.push(Box::new(FailingCheckPF {
            check_names: filter_args.failing_check.clone(),
        }));
    }
    if let Some(title) = &filter_args.title {
        out.push(Box::new(TitlePF::new().with_value(title.clone())));
    }

    out
}

fn format_user_query(query: &str) -> Result<String> {
    let mut final_query = query.to_string();

    if !final_query.contains("is:pr") {
        final_query = format!("{} is:pr", final_query);
    }

    if !final_query.contains("is:open") && !final_query.contains("is:closed") {
        final_query = format!("{} is:open", final_query);
    }

    Ok(final_query)
}

fn parse_throttle_duration(throttle_str: &str) -> Result<Duration> {
    let throttle_str = throttle_str.trim();

    if let Ok(minutes) = throttle_str.parse::<u64>() {
        return Ok(Duration::from_secs(minutes * 60));
    }

    if let Some(seconds_str) = throttle_str.strip_suffix('s') {
        let seconds: u64 = seconds_str
            .parse()
            .with_context(|| format!("Invalid throttle seconds: '{}'", seconds_str))?;
        return Ok(Duration::from_secs(seconds));
    }

    if let Some(minutes_str) = throttle_str.strip_suffix('m') {
        let minutes: u64 = minutes_str
            .parse()
            .with_context(|| format!("Invalid throttle minutes: '{}'", minutes_str))?;
        return Ok(Duration::from_secs(minutes * 60));
    }

    if let Some(hours_str) = throttle_str.strip_suffix('h') {
        let hours: u64 = hours_str
            .parse()
            .with_context(|| format!("Invalid throttle hours: '{}'", hours_str))?;
        return Ok(Duration::from_secs(hours * 3600));
    }

    anyhow::bail!(
        "Invalid throttle format '{}'. Supported formats: unitless number (minutes), '30s', '5m', '2h'",
        throttle_str
    )
}

fn validate_pr_urls_against_repo(repo: Option<&str>, prs: &[String]) -> Result<()> {
    let Some(repo) = repo else {
        return Ok(());
    };

    let expected_repo = Repo::parse(repo)
        .map_err(|e| anyhow::anyhow!("Invalid repository format '{}': {}", repo, e))?;

    for pr in prs {
        if pr.starts_with("https://") {
            let (pr_repo, _) = Repo::parse_url(pr)?;
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

fn parse_pr_args_to_identifiers(repo: &Option<String>, prs: &[String]) -> Result<Vec<(Repo, u64)>> {
    let mut identifiers = Vec::new();

    for pr in prs {
        let pr = pr.trim(); // Trim whitespace from each value
        if pr.is_empty() {
            continue; // Skip empty strings silently (no-op)
        }
        if pr.starts_with("https://") {
            let (repo, pr_number) = Repo::parse_url(pr)?;
            let pr_number = pr_number
                .ok_or_else(|| anyhow::anyhow!("URL must contain '/pull/' in the path"))?;
            identifiers.push((repo, pr_number));
        } else {
            let Some(repo) = repo else {
                anyhow::bail!("PR numbers require --repo to be specified");
            };
            let repo_id = Repo::parse(repo)
                .map_err(|e| anyhow::anyhow!("Invalid repository format '{}': {}", repo, e))?;

            let pr_number: u64 = pr
                .parse()
                .with_context(|| format!("Invalid PR number: '{}'", pr))?;

            identifiers.push((repo_id, pr_number));
        }
    }

    Ok(identifiers)
}

fn determine_display_mode(cli: &CliArgs) -> DisplayMode {
    match (cli.quiet, cli.detailed, cli.detailed_with_logs) {
        (true, _, _) => DisplayMode::Quiet,
        (_, _, true) => DisplayMode::DetailedWithLogs,
        (_, true, _) => DisplayMode::Detailed,
        _ => DisplayMode::Normal,
    }
}

fn create_autoprat_request(cli: CliArgs) -> Result<QuerySpec> {
    cli.validate()?;

    let repo = cli
        .repo
        .as_ref()
        .map(|r| {
            Repo::parse(r).map_err(|e| anyhow::anyhow!("Invalid repository format '{}': {}", r, e))
        })
        .transpose()?;

    validate_pr_urls_against_repo(cli.repo.as_deref(), &cli.prs)?;
    validate_pr_urls_against_repo(cli.repo.as_deref(), &cli.exclude)?;
    let pr_identifiers = parse_pr_args_to_identifiers(&cli.repo, &cli.prs)?;
    let exclude_identifiers = parse_pr_args_to_identifiers(&cli.repo, &cli.exclude)?;

    let query = cli
        .query
        .as_ref()
        .map(|q| format_user_query(q))
        .transpose()?;

    let throttle = cli
        .throttle
        .as_ref()
        .filter(|t| !t.trim().is_empty())
        .map(|t| parse_throttle_duration(t))
        .transpose()?;

    Ok(QuerySpec {
        repo,
        prs: pr_identifiers,
        exclude: exclude_identifiers,
        query,
        limit: cli.limit,
        search_filters: cli_to_search_filters(&cli.filters),
        post_filters: cli_to_post_filters(&cli.filters),
        actions: cli_to_actions(&cli.actions),
        custom_comments: cli.comment,
        throttle,
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
            _ => arg,
        })
        .collect()
}

fn build_query_from_cli(cli: CliArgs) -> Result<(QuerySpec, DisplayMode)> {
    let display_mode = determine_display_mode(&cli);
    let request = create_autoprat_request(cli)?;
    Ok((request, display_mode))
}

/// Parses command-line arguments into a query specification and display mode.
///
/// Transforms slash commands (e.g., /retest) into standard arguments and
/// validates all inputs according to CLI rules. Returns structured query
/// parameters ready for execution.
pub fn parse_args<I, T>(args: I) -> Result<(QuerySpec, DisplayMode)>
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
