# autoprat: **Autonomous Pull-Request Automation**

Automate the tedium out of your GitHub workflow.

No more opening PRs in a browser to type `/lgtm`, `/approve`, `/ok-to-test`, `/test`, `/retest` or `/override ci/prow/...` by hand. Let **autoprat** handle it with just enough contempt for the process to keep you sane.

---

## Installation

1. Clone or download this repo:

   ```bash
   git clone https://github.com/frobware/autoprat.git
   cd autoprat
   chmod +x autoprat
   cp autoprat $HOME/bin/autoprat
   ```

2. Install prerequisites:

   - GitHub CLI (`gh`)
   - `jq`

3. Authenticate `gh` if you haven't already:

   ```bash
   gh auth login
   ```

---

## Why autoprat?

You are tired of:

- Clicking into each bot-generated PR just to type `/lgtm`
- Remembering whether to use `/approve` or `/ok-to-test` or `/override`
- Stripping off `ci/prow/` prefixes for `/test` commands
- Copy/pasting job names across multiple repositories

**autoprat** does it all in one command. You choose the flags; it chooses the right comment, idempotently. No more manual drudgery.

---

## Exploring PRs with autoprat

**autoprat** includes a built-in list mode that makes it easy to explore PRs before taking action. Use the `--list` option to see what PRs are available and which ones need attention.

### Basic listing with autoprat

List all open PRs in a repository:

```bash
autoprat -r openshift/bpfman-operator --list
```

Get detailed information about a specific PR:

```bash
autoprat -r openshift/bpfman-operator --list 488
```

Example output:
```
#489 - app/red-hat-konflux - chore(deps): update ocp-bpfman-operator to 65b0d10
  State:   OPEN | Created: 2025-05-12
  URL:     https://github.com/openshift/bpfman-operator/pull/489
  Status:
    - Approved: ✗
    - CI: ✗ Failing
    - LGTM: ✗
    - OK-to-test: ✓
  Labels:
    - konflux-nudge
    - needs-ok-to-test
  Checks:
    - Red Hat Konflux / bpfman-operator-bundle-on-pull-request: SUCCESS
    - Red Hat Konflux / bpfman-operator-enterprise-contract / ocp-bpfman-operator-bundle: FAILURE
    - tide: PENDING
```

### Finding PRs that need attention

Find PRs that need approval or LGTM:

```bash
# Find PRs that need approval
autoprat -r openshift/bpfman-operator --list --needs-approve

# Find PRs that need LGTM
autoprat -r openshift/bpfman-operator --list --needs-lgtm

# Find PRs needing both
autoprat -r openshift/bpfman-operator --list --needs-approve --needs-lgtm
```

### Filtering by author

Filter PRs by author using exact match or regex patterns:

```bash
# Exact match for a specific author
autoprat -r openshift/bpfman-operator --list --author "app/red-hat-konflux"

# Regex pattern to find bot authors
autoprat -r openshift/bpfman-operator --list --author ".*bot.*"
```

### Advanced usage

Combine filters for targeted searching:

```bash
# Find PRs from automated accounts that need approval
autoprat -r openshift/bpfman-operator --list --author "app/red-hat-konflux" --needs-approve

# Check CI status of PRs matching a pattern
autoprat -r openshift/bpfman-operator --list --author ".*konflux.*"
```


### Complete workflow example

Here's a complete workflow using autoprat:

```bash
# 1. Find PRs that need approval
autoprat -r openshift/bpfman-operator --list --needs-approve

# 2. Check which of those are from automation accounts
autoprat -r openshift/bpfman-operator --list --needs-approve --author "app/red-hat-konflux"

# 3. Get detailed view of a specific PR
autoprat -r openshift/bpfman-operator --list --needs-approve --author "app/red-hat-konflux" 489

# 4. Approve those PRs (with dry-run first)
autoprat -r openshift/bpfman-operator -a -n --author "app/red-hat-konflux" --approve

# 5. If everything looks good, run without -n to actually post the comments
# autoprat -r openshift/bpfman-operator -a --author "app/red-hat-konflux" --approve
```

---

## Examples

### Single-PR operations

- Give it your approval and LGTM:

  ```bash
  autoprat -r OWNER/REPO --lgtm --approve 123
  ```

- Grant ok-to-test:

  ```bash
  autoprat -r OWNER/REPO --ok-to-test 123
  ```

- Re-run just the `test-fmt` job:

  ```bash
  autoprat -r OWNER/REPO 123 -j ci/prow/test-fmt
  # posts: /test test-fmt
  ```

- Override a context:

  ```bash
  autoprat -r OWNER/REPO 123 -x /override -j ci/prow/test-fmt
  # posts: /override ci/prow/test-fmt
  ```

- Post a bare `/retest`:

  ```bash
  autoprat -r OWNER/REPO -c /retest 123
  # posts: /retest
  ```

### Bulk operations (all open PRs)

- Approve, LGTM and OK-to-test every open PR:

  ```bash
  autoprat -r OWNER/REPO -a --lgtm --approve --ok-to-test
  ```

- Filter PRs by author (exact match):

  ```bash
  # Approve all PRs from a specific automation account
  autoprat -r openshift/bpfman-operator -a --author "app/red-hat-konflux" --approve
  ```

- Filter PRs by author (regex pattern):

  ```bash
  # Add lgtm to all PRs from authors with "konflux" in their name
  autoprat -r openshift/bpfman-operator -a --author ".*konflux.*" --lgtm
  ```

- Add a comment to all PRs from a specific author:

  ```bash
  # Post a retest comment on all PRs from the automation account
  autoprat -r openshift/bpfman-operator -a -n --author "app/red-hat-konflux" -c "/retest"
  ```

- Re-trigger specific CI jobs for PRs from a specific author:

  ```bash
  # Re-trigger a failing CI job for all PRs from the automation account
  autoprat -r openshift/bpfman-operator -a -n --author "app/red-hat-konflux" -j "ocp-bpfman-operator-bundle"
  ```

- Re-run multiple jobs on every PR from a specific author and leave a note:

  ```bash
  autoprat -r openshift/bpfman-operator -a -n \
    --author "app/red-hat-konflux" \
    -j ocp-bpfman-operator-bundle \
    -j bpfman-operator-bundle-on-pull-request \
    -x /retest \
    -c "Re-running CI jobs - automated message"
  ```

- Dry-run to verify what would happen:

  ```bash
  autoprat -r openshift/bpfman-operator -a -n \
    --author "app/red-hat-konflux" \
    --ok-to-test
  ```

---

## License

MIT License.