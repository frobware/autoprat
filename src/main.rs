mod commands;
mod display;
mod github;
mod graphql;
mod search;
mod types;

use anyhow::{Context, Result};
use clap::Parser;

use crate::{
    commands::{generate_action_commands, has_action_commands, output_commands},
    display::display_prs_by_mode,
    github::setup_github_client,
    search::{
        apply_remaining_filters, build_search_query_from_cli, collect_specific_prs,
        process_prs_streaming,
    },
};

// Human-readable build info (for clap version display)
const BUILD_INFO_HUMAN: &str = env!("BUILD_INFO_HUMAN");

#[derive(Parser)]
#[command(name = "autoprat")]
#[command(
    about = "Stop clicking through GitHub PRs one by one - finds PRs you care about and generates commands to act on them in bulk (approve, LGTM, retest, close, etc.)"
)]
#[command(long_version = BUILD_INFO_HUMAN)]
pub struct Cli {
    /// GitHub repository in format 'owner/repo' (required when using numeric PR
    /// arguments or no PR arguments)
    #[arg(short = 'r', long = "repo")]
    pub repo: Option<String>,

    /// PR numbers or URLs to focus on (can specify multiple)
    #[arg(help = "PR-NUMBER|PR-URL ...")]
    pub prs: Vec<String>,

    /// Exact author match
    #[arg(short = 'a', long = "author")]
    pub author: Option<String>,

    /// Has label (prefix - to negate, can specify multiple)
    #[arg(long)]
    pub label: Vec<String>,

    /// Has failing CI checks
    #[arg(long = "failing-ci")]
    pub failing_ci: bool,

    /// Specific CI check is failing (exact match)
    #[arg(long = "failing-check", value_name = "NAME")]
    pub failing_check: Vec<String>,

    /// Missing 'approved' label
    #[arg(long = "needs-approve")]
    pub needs_approve: bool,

    /// Missing 'lgtm' label
    #[arg(long = "needs-lgtm")]
    pub needs_lgtm: bool,

    /// Has 'needs-ok-to-test' label
    #[arg(long = "needs-ok-to-test")]
    pub needs_ok_to_test: bool,

    /// Raw GitHub search query (mutually exclusive with filter options)
    #[arg(long, conflicts_with_all = ["repo", "prs", "author", "label", "failing_ci", "failing_check", "needs_approve", "needs_lgtm", "needs_ok_to_test"])]
    pub query: Option<String>,

    /// Generate /approve comments
    #[arg(long)]
    pub approve: bool,

    /// Generate /lgtm comments
    #[arg(long)]
    pub lgtm: bool,

    /// Generate /ok-to-test comments
    #[arg(long = "ok-to-test")]
    pub ok_to_test: bool,

    /// Close PRs
    #[arg(long)]
    pub close: bool,

    /// Generate /retest comments
    #[arg(long)]
    pub retest: bool,

    /// Generate custom comment commands (can specify multiple)
    #[arg(short = 'c', long)]
    pub comment: Vec<String>,

    /// Skip if same comment posted recently (e.g. 5m, 1h)
    #[arg(long, value_name = "DURATION")]
    pub throttle: Option<String>,

    /// Show detailed PR information
    #[arg(short = 'd', long = "detailed")]
    pub detailed: bool,

    /// Show detailed PR information with error logs from failing checks
    #[arg(short = 'D', long = "detailed-with-logs")]
    pub detailed_with_logs: bool,

    /// Print PR numbers only
    #[arg(short = 'q', long = "quiet")]
    pub quiet: bool,

    /// Enable debug logging
    #[arg(long)]
    pub debug: bool,

    /// Limit the number of PRs to process
    #[arg(short = 'L', long, default_value_t = 30)]
    pub limit: usize,
}

/// Handles repository-wide PR search workflow using GitHub GraphQL streaming.
async fn handle_repository_search(octocrab: &octocrab::Octocrab, cli: &Cli) -> Result<()> {
    let search_query = if cli.query.is_some() {
        // For raw queries, we don't need a repo parameter
        build_search_query_from_cli("", cli)?
    } else {
        let repo = cli
            .repo
            .as_ref()
            .context("Repository (--repo) is required when no PR arguments are specified")?;
        build_search_query_from_cli(repo, cli)?
    };
    let (action_commands, all_prs) = process_prs_streaming(octocrab, &search_query, cli).await?;

    if has_action_commands(cli) {
        output_commands(action_commands);
    } else {
        display_prs_by_mode(&all_prs, cli)?;
    }

    Ok(())
}

/// Handles workflow for processing specific PRs provided via CLI arguments.
async fn handle_specific_prs(octocrab: &octocrab::Octocrab, cli: &Cli) -> Result<()> {
    let all_prs = collect_specific_prs(octocrab, cli).await?;
    let filtered_prs = apply_remaining_filters(all_prs, cli);

    if has_action_commands(cli) {
        let commands = generate_action_commands(&filtered_prs, cli)?;
        output_commands(commands);
    } else {
        display_prs_by_mode(&filtered_prs, cli)?;
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let octocrab = setup_github_client().await?;

    if cli.prs.is_empty() {
        handle_repository_search(&octocrab, &cli).await
    } else {
        handle_specific_prs(&octocrab, &cli).await
    }
}
