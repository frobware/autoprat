# autoprat: Pull Request Automation Tool

Automate GitHub PR comment workflows from the command line.

No more opening PRs in a browser to type `/lgtm`, `/approve`, `/ok-to-test`, or `/retest`. Let autoprat generate the commands for you.

---

## How it Works

autoprat filters pull requests and generates `gh` CLI commands for common actions:

1. **Filter** PRs based on criteria (author, labels, CI status)
2. **Generate** `gh` commands for the actions you want
3. **Output** commands for review and execution

**Note**: autoprat only generates commands. To execute them, pipe to `sh`:

```bash
autoprat -r owner/repo --needs-approve --approve --print | sh
```

---

## Installation

### Prerequisites

- `gh` (GitHub CLI) - must be authenticated
- Go 1.24+ (to build from source)

### Install

Using `go install`:

```bash
go install github.com/frobware/autoprat@latest
```

Or clone and build:

```bash
git clone https://github.com/frobware/autoprat.git
cd autoprat
go build -o autoprat .
```

---

## Core Concepts

### Filters vs Actions

autoprat separates filtering from actions:

- **Filters** select PRs to work with
- **Actions** define what commands to generate

### Filter Options

```
--author <exact>        Filter by author (exact match)
--author-fuzzy <fuzzy>  Filter by author (substring match)
--label <label>         Filter by label (prefix with ! to negate)
--failing-ci            Only PRs with failing CI
--needs-approve         Only PRs missing 'approved' label
--needs-lgtm            Only PRs missing 'lgtm' label
--needs-ok-to-test      Only PRs with 'needs-ok-to-test' label
```

### Action Options

```
--approve               Post /approve comment
--lgtm                  Post /lgtm comment
--ok-to-test            Post /ok-to-test comment
--comment <text>        Post custom comment
```

### Output Modes

```
(default)               Show PR table
--print, -P             Print gh commands instead of PR list
--verbose, -v           Show detailed PR information with clickable/copyable log URLs
-V                      Show detailed PR information with automatic error log extraction
--quiet, -q             Show PR numbers only
--no-hyperlinks         Force explicit URLs (useful for terminals without hyperlink support)
```

---

## Examples

### Basic Workflow

1. **Find PRs that need approval:**
   ```bash
   autoprat -r openshift/bpfman-operator --needs-approve
   ```

2. **Generate approval commands:**
   ```bash
   autoprat -r openshift/bpfman-operator --needs-approve --approve --print
   ```

3. **Execute commands:**
   ```bash
   autoprat -r openshift/bpfman-operator --needs-approve --approve --print | sh
   ```

### Filtering Examples

```bash
# PRs from specific author
autoprat -r owner/repo --author "red-hat-konflux"

# PRs from authors containing "bot"
autoprat -r owner/repo --author-fuzzy "bot"

# PRs with specific label
autoprat -r owner/repo --label "kind/bug"

# PRs WITHOUT a label (negation)
autoprat -r owner/repo --label "!do-not-merge"

# Combine filters (AND logic)
autoprat -r owner/repo --author "dependabot" --needs-lgtm --failing-ci

# Multiple labels (must have all)
autoprat -r owner/repo --label "kind/bug" --label "priority/high"

# Multiple comments on matched PRs
autoprat -r owner/repo --failing-ci \
  --comment "CI is failing" \
  --comment "Please check the logs" \
  --print | sh
```

### Action Examples

```bash
# Approve all PRs from dependabot
autoprat -r owner/repo --author "dependabot" --approve --print | sh

# LGTM and approve PRs missing both
autoprat -r owner/repo --needs-lgtm --needs-approve --lgtm --approve --print | sh

# Comment on failing PRs
autoprat -r owner/repo --failing-ci --comment "Investigating CI failure" --print | sh

# Give ok-to-test to PRs that need it
autoprat -r owner/repo --needs-ok-to-test --ok-to-test --print | sh
```

### Advanced Examples

```bash
# Dry run - see what commands would be generated
autoprat -r owner/repo --needs-approve --approve --print

# Multiple actions on filtered PRs
autoprat -r owner/repo \
  --author "red-hat-konflux" \
  --needs-lgtm \
  --needs-approve \
  --lgtm \
  --approve \
  --comment "Automated approval" \
  --print | sh

# Get PR numbers for scripting
autoprat -r owner/repo --failing-ci --quiet > failing-prs.txt

# View specific PR details
autoprat -r owner/repo --verbose 123
```

### Log Viewing Examples

```bash
# View PR status with log access (smart hyperlinks/URLs based on terminal)
autoprat -r owner/repo --verbose

# View PR status with automatic error extraction from failing checks
autoprat -r owner/repo -V

# Force explicit URLs (useful for Alacritty or copy/paste workflows)
autoprat -r owner/repo --verbose --no-hyperlinks

# Focus on failures with detailed error logs
autoprat -r owner/repo --failing-ci -V

# Check specific failing PR with error extraction
autoprat -r owner/repo -V 123
```

---

## Command Reference

```
Usage:
  autoprat [flags] [PR-NUMBER...]

Required:
  -r, --repo OWNER/REPO     GitHub repository

Filters:
  -a, --author EXACT        Filter by author (exact match)
  -A, --author-fuzzy FUZZY  Filter by author (substring)
  -l, --label LABEL         Filter by label (! prefix to negate)
  -f, --failing-ci          Only PRs with failing CI
  --needs-approve           Only PRs missing 'approved' label
  --needs-lgtm              Only PRs missing 'lgtm' label
  --needs-ok-to-test        Only PRs with 'needs-ok-to-test' label

Actions:
  --approve                 Generate /approve commands
  --lgtm                    Generate /lgtm commands
  --ok-to-test              Generate /ok-to-test commands
  -c, --comment TEXT        Generate custom comment commands

Output:
  -P, --print               Print gh commands (required for actions)
  -v, --verbose             Show PR details with clickable/copyable log URLs
  -V, --verbose-verbose     Show PR details with automatic error log extraction
  -q, --quiet               Show PR numbers only
  --no-hyperlinks           Force explicit URLs (for terminals without hyperlink support)

Positional:
  [PR-NUMBER...]            Specific PR numbers to process
```

---

## Terminal Compatibility

autoprat automatically detects your terminal's capabilities and provides the best log access experience:

### Hyperlink Support
- **iTerm2, GNOME Terminal, Windows Terminal, VS Code**: Clickable status text (SUCCESS/FAILURE become hyperlinks)
- **Alacritty, older terminals**: Explicit URLs shown for copy/paste
- **Override**: Use `--no-hyperlinks` to force explicit URLs

### Automatic Detection
autoprat checks environment variables (`TERM`, `TERM_PROGRAM`, etc.) to determine hyperlink support. No configuration needed.

---

## Implementation

autoprat uses GitHub's GraphQL API to fetch PR data, applies filters in-memory, and generates `gh pr comment` commands.

Features:
- Single API call for all PR data
- Explicit filter and action separation
- Standard Unix pipe compatibility
- Command preview before execution
- Smart terminal detection for optimal log access
- Automatic error extraction from CI logs

---

## Common Workflows

### Daily PR Maintenance

```bash
# Check what needs attention
autoprat -r myorg/myrepo --needs-approve

# Approve PRs from trusted bots
autoprat -r myorg/myrepo --author "dependabot" --approve --print | sh
autoprat -r myorg/myrepo --author "renovate" --approve --print | sh

# Handle PRs needing ok-to-test
autoprat -r myorg/myrepo --needs-ok-to-test --ok-to-test --print | sh
```

### CI Failure Investigation

```bash
# List failing PRs
autoprat -r myorg/myrepo --failing-ci

# View failure details with log URLs
autoprat -r myorg/myrepo --failing-ci --verbose

# View failures with automatic error extraction
autoprat -r myorg/myrepo --failing-ci -V

# Comment on failing PRs
autoprat -r myorg/myrepo --failing-ci \
  --comment "CI is failing, investigating..." --print | sh
```

### Bulk Operations

```bash
# Step 1: See what needs both LGTM and approval
autoprat -r myorg/myrepo --needs-lgtm --needs-approve

# Step 2: Filter by author if needed
autoprat -r myorg/myrepo --needs-lgtm --needs-approve \
  --author "trusted-contributor"

# Step 3: Review commands
autoprat -r myorg/myrepo --needs-lgtm --needs-approve \
  --author "trusted-contributor" --lgtm --approve --print

# Step 4: Execute
autoprat -r myorg/myrepo --needs-lgtm --needs-approve \
  --author "trusted-contributor" --lgtm --approve --print | sh
```

---

## Tips

1. **Review first**: Use `--print` without `| sh` to see commands
2. **Multiple filters**: Filters combine with AND logic
3. **Scripting**: Use `--quiet` to get PR numbers only
4. **Test filters**: Run without actions to see matched PRs
5. **Start small**: Test on individual PRs before bulk operations

---

## License

MIT License.
