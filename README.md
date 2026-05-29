# autoprat

`autoprat` finds GitHub pull requests and, when asked to act, prints the `gh` commands to do it.

It is for maintainers who already live in GitHub, Prow-style slash commands, bot PRs, CI labels, and bulk triage.

**autoprat never changes a pull request itself.** It only queries GitHub and writes output to stdout. With action flags, that output is `gh` commands. Nothing is approved, held, closed, merged, or commented on until you run those commands yourself -- usually by piping them to `sh` after reading them. Reviewing the output before running it is the point of the tool.

## Install

Prerequisites:

- `gh` installed and authenticated
- Rust, if installing from source

```bash
cargo install --git https://github.com/frobware/autoprat.git
```

From a checkout:

```bash
cargo build --release
```

Use `autoprat --help` for the current flag list.

## Basic Use

List matching PRs:

```bash
autoprat -r org/repo --needs-approve
autoprat -r org/repo --failing-ci
autoprat -r org/repo --author dependabot
```

Generate commands:

```bash
autoprat -r org/repo --author dependabot --approve
```

Run them after reviewing:

```bash
autoprat -r org/repo --author dependabot --approve | sh
```

Prow-style action names work as positional slash commands too:

```bash
autoprat -r org/repo /approve
autoprat -r org/repo /lgtm
autoprat -r org/repo /ok-to-test
autoprat -r org/repo /retest
autoprat -r org/repo /hold
```

`/close` and `/merge` are accepted as well.

## Selecting PRs

Work from a repository:

```bash
autoprat -r org/repo --needs-lgtm
autoprat -r org/repo --needs-approve --author alice
```

Target specific PRs:

```bash
autoprat -r org/repo 123 456
autoprat -r org/repo 123-127
autoprat https://github.com/org/repo/pull/123
```

Ranges are inclusive. `123-127` means `123 124 125 126 127`. Add `-d` for the detailed view of the selected PRs.

Exclude PRs from a wider selection:

```bash
autoprat -r org/repo --needs-approve --exclude 123,456 --approve
autoprat -r org/repo --needs-lgtm --exclude 120-130 --lgtm
autoprat -r org/repo --exclude https://github.com/org/repo/pull/789 --hold
```

Search multiple repositories:

```bash
autoprat -r org/backend -r org/frontend --failing-ci
autoprat -r org/repo1 -r org/repo2 --author dependabot --approve
```

Or use PR URLs when the selection spans repositories:

```bash
autoprat \
  https://github.com/org/backend/pull/123 \
  https://github.com/org/frontend/pull/456
```

## Common Workflows

Approve trusted bot PRs:

```bash
autoprat -r org/repo --author dependabot --approve
autoprat -r org/repo --author dependabot --approve | sh
```

Handle PRs waiting for test permission:

```bash
autoprat -r org/repo --needs-ok-to-test
autoprat -r org/repo --needs-ok-to-test --ok-to-test | sh
```

Find PRs missing merge preconditions:

```bash
autoprat -r org/repo --needs-approve
autoprat -r org/repo --needs-lgtm
autoprat -r org/repo --needs-approve --needs-lgtm
```

Retest failing PRs:

```bash
autoprat -r org/repo --failing-ci
autoprat -r org/repo --failing-ci --retest | sh
```

Target one failing check:

```bash
autoprat -r org/repo --failing-check "ci/test-flaky"
autoprat -r org/repo --failing-check "ci/test-flaky" \
  --comment "/override ci/test-flaky" | sh
```

Hold a set of PRs:

```bash
autoprat -r org/repo --title "(?i)api" --hold
autoprat -r org/repo --title "(?i)api" --hold | sh
```

Merge PRs that already satisfy your labels:

```bash
autoprat -r org/repo --label approved --label lgtm --merge
```

Find bot PRs that are not single-commit updates:

```bash
autoprat -r org/repo --author red-hat-konflux --commits ">1"
autoprat -r org/repo --author red-hat-konflux --commits 1 --lgtm
```

Use a raw GitHub search query when the built-in filters are not enough:

```bash
autoprat --query "repo:org/repo author:dependabot created:>2026-01-01"
autoprat --query "repo:org/repo status:failure comments:>5"
```

`is:pr` and `is:open` are added to raw queries when you do not specify them.

## Safety

autoprat never acts on a PR itself; in action mode it only prints the `gh` commands, and piping to `sh` is the only step that changes anything, so nothing happens to a PR until you choose to run the output. The normal workflow is to look before you run:

```bash
# 1. Inspect the matching PRs.
autoprat -r org/repo --needs-approve

# 2. Inspect the commands.
autoprat -r org/repo --needs-approve --approve

# 3. Run them.
autoprat -r org/repo --needs-approve --approve | sh
```

Built-in comment actions avoid obvious duplicates:

- `--approve` only comments when the PR does not already have `approved`
- `--lgtm` only comments when the PR does not already have `lgtm`
- `--ok-to-test` only comments when the PR has `needs-ok-to-test`
- `--hold` only comments when the PR does not already have `do-not-merge/hold`

`--close`, `--merge`, `--retest`, and custom `--comment` actions are direct requests.

`--throttle` suppresses a comment if the same body was posted recently:

```bash
autoprat -r org/repo --failing-ci --comment "/retest" --throttle 30m | sh
```

The commit limit guard stops you acting blindly on a bot PR that carries more commits than you would expect. When an action is requested, autoprat refuses to emit commands if any targeted PR has more commits than `--commit-limit` allows. The default is `1`: routine bot updates (Dependabot, Konflux, and the like) are almost always a single commit, so a targeted PR with more is unusual and worth a look before you act on it.

```bash
autoprat -r org/repo --author red-hat-konflux --commits ">1"
autoprat -r org/repo --author red-hat-konflux --lgtm --commit-limit 50 | sh
```

Use `--exclude` to skip individual PRs instead of raising the limit for everything.

## Output

Without action flags, autoprat prints matching PRs.

Useful views:

```bash
autoprat -r org/repo --failing-ci
autoprat -r org/repo --failing-ci -d
autoprat -r org/repo --failing-ci -D
autoprat -r org/repo --quiet
```

`-d` shows a detailed PR tree. `-D` also tries to fetch error logs for failing checks. `--quiet` prints PR numbers only.

When stdout is not a terminal, the default table becomes tab-separated output with no header. Boolean columns are `1`/`0`, and timestamps are RFC3339. Use this for scripts.

```bash
autoprat -r org/repo --needs-approve > prs.tsv
```

Set `AUTOPRAT_FORCE_TTY=1` if you want the human table while piping:

```bash
AUTOPRAT_FORCE_TTY=1 autoprat -r org/repo --failing-ci | less -R
```

Use `-S` when watching output in a narrow terminal:

```bash
watch -n 180 'autoprat -r org/repo --failing-ci -S'
```

## CI Status

The CI column is compressed so it fits in a table.

Examples:

- `Success`: all checks passed
- `Failed: 1/2`: one of two checks failed
- `F:2 C:1 (3/5)`: two failed, one cancelled, five total
- `S:4 F:0 X:1 Q:2 (4/7)`: four succeeded, one running, two queued
- `Unknown`: no usable check state

Prow merge-prerequisite states are not treated as active CI. Use the label columns to see `approved`, `lgtm`, `needs-ok-to-test`, and hold state.

## Debugging

Tracing uses `RUST_LOG`:

```bash
RUST_LOG=autoprat=debug autoprat -r org/repo -D
RUST_LOG=autoprat::github=debug autoprat -r org/repo
RUST_LOG=autoprat::log_fetcher=debug autoprat -r org/repo -D
```

## Development

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```

## License

MIT
