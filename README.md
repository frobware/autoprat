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

## Exploring PRs for autoprat

Before using `autoprat`, you may want to explore the PRs in a repository to understand who the authors are or which PRs need attention. Here are some helpful commands using real examples from the openshift/bpfman-operator repo:

### Listing PRs with authors

To list all open PRs in a repository with their authors:

```bash
gh pr list --repo openshift/bpfman-operator --json number,author,title | jq -r '.[] | "\(.number) - \(.author.login) - \(.title)"'
```

Example output:
```
489 - app/red-hat-konflux - chore(deps): update ocp-bpfman-operator to 65b0d10
488 - app/red-hat-konflux - chore(deps): update ocp-bpfman-agent to 0527d3b
487 - app/red-hat-konflux - fix(deps): update github.com/openshift/api digest to b7d0ca2
```

### Finding unique PR authors

To see all unique authors with open PRs:

```bash
gh pr list --repo openshift/bpfman-operator --json author | jq -r '.[] | .author.login' | sort | uniq
```

Example output:
```
app/red-hat-konflux
```

### Filtering PRs by content

Find PRs that contain specific patterns in their titles:

```bash
# Find dependency update PRs
gh pr list --repo openshift/bpfman-operator --json number,author,title | \
  jq -r '.[] | select(.title | test("chore\\(deps\\)")) | "\(.number) - \(.author.login) - \(.title)"'
```

Example output:
```
489 - app/red-hat-konflux - chore(deps): update ocp-bpfman-operator to 65b0d10
488 - app/red-hat-konflux - chore(deps): update ocp-bpfman-agent to 0527d3b
485 - app/red-hat-konflux - chore(deps): update google.golang.org/genproto/googleapis/rpc digest to f936aa4
```

### Checking PR status and CI jobs

Check CI status for a specific PR:

```bash
gh pr checks 489 --repo openshift/bpfman-operator
```

Example output:
```
Red Hat Konflux / bpfman-operator-enterprise-contract / ocp-bpfman-operator-bundle	fail	1s
Red Hat Konflux / bpfman-operator-bundle-on-pull-request	pass	2m54s
tide	pending	0	Not mergeable. Needs approved, lgtm labels.
```

### Real-world workflow example

Here's a complete workflow example using a real repository:

```bash
# 1. First, find PRs that need approval
NEEDS_APPROVAL=$(gh pr list --repo openshift/bpfman-operator --json number,author,title,labels | \
  jq -r '.[] | select(.labels | map(.name) | contains(["approved"]) | not) | "\(.number) - \(.author.login) - \(.title)"')

# 2. Display them for review
echo "$NEEDS_APPROVAL"

# Output:
# 489 - app/red-hat-konflux - chore(deps): update ocp-bpfman-operator to 65b0d10
# 488 - app/red-hat-konflux - chore(deps): update ocp-bpfman-agent to 0527d3b

# 3. Filter to find automation PRs that need approval
echo "$NEEDS_APPROVAL" | grep "app/red-hat-konflux"

# 4. Use autoprat to approve just those PRs (with dry-run first)
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