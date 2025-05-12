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

3. Authenticate `gh` if you haven’t already:

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

- Re-run two jobs on every broken PR and leave a note:

  ```bash
  autoprat -r OWNER/REPO -a \
    -j ci/prow/test-fmt \
    -j ci/prow/security \
    -x /retest \
    -c "please re-run CI; I’m done typing this by hand"
  ```

- Dry-run to verify what would happen:

  ```bash
  autoprat -r OWNER/REPO -a -n \
    --ok-to-test

  autoprat -r OWNER/REPO -a -n \
    --lgtm --approve
  ```

---

## License

MIT License.
