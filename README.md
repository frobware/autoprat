# autoprat

**Stop clicking through GitHub PRs one by one.**

autoprat finds the PRs you care about and generates the commands to act on them in bulk.

## The Problem

You maintain a busy repository. Every day you need to:
- Approve PRs from trusted contributors like Dependabot
- Give `/ok-to-test` to PRs that need it
- Comment on failing PRs to restart CI
- Find PRs missing reviews

Opening each PR in a browser tab gets old fast.

## The Solution

```bash
# Find PRs that need approval.
autoprat -r owner/repo --needs-approve

# Generate approval commands for Dependabot PRs.
autoprat -r owner/repo --author dependabot --approve
gh pr comment 123 --repo owner/repo --body "/approve"
gh pr comment 456 --repo owner/repo --body "/approve"

# Execute those commands.
autoprat -r owner/repo --author dependabot --approve | sh
```

autoprat queries GitHub once, applies your filters, and outputs standard `gh` commands you can review before running.

## Quick Start

```bash
# Install.
cargo install --git https://github.com/frobware/autoprat.git

# See what needs your attention.
autoprat -r your-org/your-repo --needs-approve --needs-lgtm

# Focus on specific PRs by number or URL.
autoprat -r your-org/your-repo --detailed 123 456
autoprat --detailed https://github.com/your-org/your-repo/pull/123

# Exclude specific PRs from processing.
autoprat -r your-org/your-repo --needs-approve --exclude 123,456 --approve
autoprat -r your-org/your-repo --exclude https://github.com/your-org/your-repo/pull/789 --lgtm

# Monitor PRs across multiple repositories.
autoprat --detailed https://github.com/org/repo1/pull/123 https://github.com/org/repo2/pull/456

# Approve trusted bot PRs.
autoprat -r your-org/your-repo --author dependabot --approve | sh

# Handle PRs needing testing permission.
autoprat -r your-org/your-repo --needs-ok-to-test --ok-to-test | sh
```

## Common Workflows

### Daily Maintenance
```bash
# What needs my attention today?
autoprat -r myorg/myrepo --needs-approve --needs-lgtm

# Bulk approve Dependabot PRs.
autoprat -r myorg/myrepo --author dependabot --approve | sh

# Give testing permission to community PRs.
autoprat -r myorg/myrepo --needs-ok-to-test --ok-to-test | sh
```

### CI Firefighting
```bash
# Find failing PRs.
autoprat -r myorg/myrepo --failing-ci

# See detailed failure info with logs.
autoprat -r myorg/myrepo --failing-ci --detailed-with-logs

# Comment on all failing PRs.
autoprat -r myorg/myrepo --failing-ci --comment "Investigating CI failures" | sh

# Retest failing PRs.
autoprat -r myorg/myrepo --failing-ci --retest | sh

# Override specific failing check across multiple PRs.
autoprat -r myorg/myrepo --failing-check "ci/test-flaky" \
  --comment "/override ci/test-flaky" | sh

# Close stale PRs with failing CI, excluding specific ones.
autoprat -r myorg/myrepo --failing-ci --author "external-contributor" \
  --exclude 123,456 --close | sh
```

### Advanced Filtering
```bash
# PRs from specific author that need LGTM.
autoprat -r myorg/myrepo --needs-lgtm --author "dependabot"

# High priority bugs without holds.
autoprat -r myorg/myrepo --label "priority/high" --label "kind/bug" --label "-do-not-merge/hold"

# PRs missing approval from specific author, excluding some.
autoprat -r myorg/myrepo --author "trusted-contributor" --needs-approve --exclude 789

# Raw GitHub search queries for complex filtering.
# Note: 'is:pr' and 'is:open' are automatically added if not present
autoprat --query "repo:myorg/myrepo author:dependabot created:>2024-01-01"
autoprat --query "repo:myorg/myrepo status:failure comments:>5"
autoprat --query "repo:myorg/myrepo label:bug -label:wontfix updated:>2024-01-01"
```

### Multi-Repository Workflows
```bash
# Monitor related PRs across multiple repositories.
autoprat --detailed \
  https://github.com/myorg/backend/pull/123 \
  https://github.com/myorg/frontend/pull/456

# Apply filters across multiple repositories.
autoprat --author dependabot --approve \
  https://github.com/myorg/repo1/pull/123 \
  https://github.com/myorg/repo2/pull/456

# Bulk approve Dependabot PRs across an organization.
autoprat --author "dependabot" --approve \
  https://github.com/myorg/backend/pull/789 \
  https://github.com/myorg/frontend/pull/101 \
  https://github.com/myorg/docs/pull/202
```

## How It Works

**Workflow:** specify repository → apply filters → choose actions → select output format

1. **Parallel API calls** fetch all open PRs from specified repositories with labels, CI status, and recent comments
2. **Filter in memory** using your criteria (author, labels, CI status, etc.) applied globally across all repositories
3. **Generate standard gh commands** that you can review before executing
4. **Execute selectively** by piping to shell or running commands individually

autoprat never executes commands itself - it only generates `gh pr comment` commands for you to review and run.

## Smart Features

### Idempotent Actions
Built-in actions are smart and safe to run repeatedly:
```bash
# Safe to run multiple times - only acts when needed.
autoprat -r myorg/myrepo --approve | sh
```

The conditional actions (`--approve`, `--lgtm`, `--ok-to-test`) check existing labels and only generate commands when appropriate, whilst `--close` and `--retest` always execute when specified. Perfect for automation - no duplicate comments, no spam.

### Comment Throttling
Prevent spam when running in loops:
```bash
# Only post if same comment wasn't posted in last 30 minutes.
autoprat -r myorg/myrepo --failing-ci \
  --comment "Restarting CI" --throttle 30m | sh

# Post multiple comments to each matching PR.
autoprat -r myorg/myrepo --failing-ci \
  --comment "Investigating failures" \
  --comment "/retest" | sh
```

### Intelligent Detailed Output
Two levels of detail for different needs:

**Basic detailed (`-d`)** - Detailed PR tree view with URLs:
```bash
# See PR status, labels, and CI check results.
autoprat -r myorg/myrepo --detailed

# Focus on failing PRs with full status tree.
autoprat -r myorg/myrepo --failing-ci --detailed
```

**Detailed with logs (`-D`)** - Same as `-d` plus automatic error log extraction:
```bash
# See WHY CI checks are failing without clicking URLs.
autoprat -r myorg/myrepo --failing-ci --detailed-with-logs

# Get immediate failure insights for triage.
autoprat -r myorg/myrepo --detailed-with-logs
```


### Safety First
Always review before executing:
```bash
# 1. See what would happen.
autoprat -r myorg/myrepo --needs-approve --approve

# 2. Execute if satisfied.
autoprat -r myorg/myrepo --needs-approve --approve | sh
```

## All Options

### Repository
- `-r, --repo <REPO>` - GitHub repository in format 'owner/repo' (required when using numeric PR arguments or no PR arguments)

### Positional Arguments
- `[PRS]...` - Focus on specific PRs by number or URL (can specify multiple)
  - Numbers: `123 456` (requires `--repo`)
  - URLs: `https://github.com/owner/repo/pull/123`
  - Mixed: `123 https://github.com/owner/repo/pull/456` (requires `--repo` for numeric args)
  - Multi-repo: `https://github.com/org/repo1/pull/123 https://github.com/org/repo2/pull/456`

### Exclusions
- `-E, --exclude <PR>` - Exclude specific PRs from processing (can specify multiple or comma-separated)
  - Numbers: `--exclude 123,456` (requires `--repo`)
  - URLs: `--exclude https://github.com/owner/repo/pull/123`
  - Mixed: `--exclude 123 --exclude https://github.com/owner/repo/pull/456`
  - Spaces friendly: `--exclude "123, 456"` (automatically trimmed)
  - Empty values ignored: `--exclude ""` or `--exclude "123,,"` (trailing commas OK)

### Filters (combine with AND logic)
- `-a, --author <AUTHOR>` - Exact author match
- `--label <LABEL>` - Has label (prefix `-` to negate, can specify multiple)
- `--failing-ci` - Has failing CI checks
- `--failing-check <FAILING_CHECK>` - Specific CI check is failing (exact match)
- `--needs-approve` - Missing 'approved' label
- `--needs-lgtm` - Missing 'lgtm' label
- `--needs-ok-to-test` - Has 'needs-ok-to-test' label
- `--query <QUERY>` - Raw GitHub search query (automatically adds `is:pr` and `is:open` if not present, mutually exclusive with all other filters and repository specification)

### Actions
- `--approve` - Generate `/approve` comments
- `--lgtm` - Generate `/lgtm` comments
- `--ok-to-test` - Generate `/ok-to-test` comments
- `--close` - Close PRs
- `--retest` - Generate `/retest` comments
- `--comment <COMMENT>` - Generate custom comment commands (can specify multiple)
- `--throttle <THROTTLE>` - Skip if same comment posted recently (e.g. `5m`, `1h`)

### Output
- `-d, --detailed` - Show detailed PR information
- `-D, --detailed-with-logs` - Show detailed PR information with error logs from failing checks
- `-q, --quiet` - Print PR numbers only
- `-L, --limit <LIMIT>` - Limit the number of PRs to process [default: 30]

### Debugging
Use the `RUST_LOG` environment variable for granular tracing:
```bash
# GitHub API operations only
RUST_LOG=autoprat::github=debug autoprat -r repo

# Error pattern matching and log analysis only
RUST_LOG=autoprat::log_fetcher=debug autoprat -r repo -D

# Rate limiting and API quota tracking only
RUST_LOG=autoprat::rate_limit=debug autoprat -r repo

# All autoprat debugging
RUST_LOG=autoprat=debug autoprat -r repo -D

# Multiple categories
RUST_LOG=autoprat::github=debug,autoprat::rate_limit=debug autoprat -r repo
```

### Other
- `-h, --help` - Print help
- `-V, --version` - Print version

## Installation

### Prerequisites
- [GitHub CLI (`gh`)](https://cli.github.com/) installed and authenticated
- Rust 1.70+ (if building from source)

### Install
```bash
# Install directly from Git (like go install).
cargo install --git https://github.com/frobware/autoprat.git

# Or build from source.
git clone https://github.com/frobware/autoprat.git
cd autoprat
cargo build --release

# The binary will be at target/release/autoprat
# You can copy it to your PATH:
cp target/release/autoprat ~/.local/bin/autoprat
```

## Tips

1. **Start with filters** - Run without action flags to see which PRs match
2. **Review before executing** - Always check generated commands first
3. **Focus on specific PRs** - Add PR numbers or URLs as arguments: `autoprat -r repo -d 123 456` or `autoprat -d https://github.com/owner/repo/pull/123`
   **Multi-repository** - Monitor PRs across repositories: `autoprat -d https://github.com/org/repo1/pull/123 https://github.com/org/repo2/pull/456`
4. **Exclude problematic PRs** - Use `--exclude` to skip specific PRs: `autoprat -r repo --approve --exclude 123,456`
5. **Use throttling** - Prevent spam with `--throttle` in automated workflows
6. **Combine filters** - Multiple filters use AND logic for precise targeting
7. **Exact check names** - Use `--failing-check` with exact CI check names for safety
8. **Script the common cases** - Save frequent filter combinations as shell aliases

## License

MIT
