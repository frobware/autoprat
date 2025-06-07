# Design

autoprat leverages GitHub's search API to efficiently filter pull requests server-side before applying local actions.

## Architecture

```
build_query(filters) → search(query) → map(actions) → output(commands)
```

## Pipeline Stages

**Build Query**: Convert filter flags to GitHub search syntax
```
--author dependabot --failing-ci → "repo:owner/repo type:pr state:open author:dependabot status:failure"
```

**Search**: Execute search via GitHub GraphQL API
```
GitHub Search API → [PR1, PR2, ..., PRN] (only matching PRs returned)
```

**Map**: Generate commands for PRs
```
[PRs] |> approve → ["gh pr comment 123 --body '/approve'", ...]
```

## Query Templates

Filters are defined as search query templates in YAML:

```yaml
# github/search/templates/embedded/needs-approve.yaml
name: "Needs Approval"
flag: "needs-approve"
description: "PRs missing approval"
query: "-label:approved"
```

Parameterized templates support value substitution:

```yaml
# github/search/templates/embedded/author.yaml
name: "Author Filter"
flag: "author"
description: "Filter by exact author name"
parameterized: true
query_template: "author:{value}"
```


## Properties

- **Efficient**: Server-side filtering reduces data transfer
- **Native**: Uses GitHub's search syntax directly
- **Extensible**: YAML templates for filters and actions
- **Safe**: Generates commands, never executes them

## Example

```bash
autoprat -r owner/repo --author dependabot --failing-ci --approve
```

1. Build query: `repo:owner/repo type:pr state:open author:dependabot status:failure`
2. GitHub returns only matching PRs
3. Generate approve commands for each PR
4. Output to stdout