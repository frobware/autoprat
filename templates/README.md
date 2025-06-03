# autoprat Templates

This directory contains Go templates for formatting autoprat output. Templates use Go's `text/template` package and are embedded at compile time using `go:embed`.

## Template Files

- `verbose.tmpl` - Template for verbose PR output (`-v` and `-V` flags)

## Tree Formatting Logic

The verbose template creates a sophisticated tree structure using Unicode box-drawing characters. Here's how it works:

### Unicode Characters Used

- `├─` - Branch connector (middle items)
- `└─` - Final branch connector (last items)
- `│` - Vertical line continuation
- `●` - Bullet point for PR title

### Multi-Level Hierarchy

The tree has **three levels of nesting**:

1. **Group Level** - Status groups (FAILURE, PENDING, SUCCESS)
2. **Item Level** - Individual CI checks within each group
3. **URL Level** - URLs for checks (when not using hyperlinks)

### Example Output Structure

```
● https://github.com/org/repo/pull/123
├─Title: Example PR (author)
├─PR #123
├─State: OPEN
├─Created: 2025-01-01T00:00:00Z
├─Status
│ ├─Approved: Yes
│ ├─CI: Failing
│ ├─LGTM: No
│ └─OK-to-test: Yes
├─Labels
│ ├─bug
│ └─priority/high
└─Checks
  ├─FAILURE (2)                    ← Group level
  │ ├─ci/test-unit                 ← Item level
  │ │   └─URL: https://...         ← URL level
  │ └─ci/test-integration          ← Last item in group
  │     └─URL: https://...         ← URL for last item
  └─SUCCESS (1)                    ← Last group
    └─ci/test-lint                 ← Last item in last group
      └─URL: https://...           ← No vertical continuation
```

### Key Algorithmic Insights

#### 1. Pre-calculation Strategy

The template calculates the total number of groups upfront:

```go
{{- $totalGroups := countGroups $checksByStatus $statusOrder}}
```

This enables "lookahead" - knowing if we're rendering the last group before we start.

#### 2. Multi-dimensional Decision Making

Each prefix depends on **multiple factors**:

- **Group position**: Are we in the last group?
- **Item position**: Are we on the last item in this group?
- **Nesting level**: What level are we rendering?

#### 3. Conditional Vertical Lines

The template chooses prefixes based on position:

```go
// Group level: Last group gets └─, others get ├─
{{- $groupPrefix := "├─"}}
{{- if eq $groupIndex $totalGroups}}{{$groupPrefix = "└─"}}{{end}}

// Item level: Consider both group and item position
{{- $itemPrefix := "│ ├─"}}
{{- if and (eq $groupIndex $totalGroups) (eq $i (sub (len $checks) 1))}}
  {{$itemPrefix = "  └─"}}  // Last item in last group - no vertical line
{{- else if eq $i (sub (len $checks) 1)}}
  {{$itemPrefix = "│ └─"}}  // Last item in non-last group - keep vertical line
{{end}}

// URL level: Even deeper nesting logic
{{- $urlPrefix := "│ │   └─"}}
{{- if and (eq $groupIndex $totalGroups) (eq $i (sub (len $checks) 1))}}
  {{$urlPrefix = "    └─"}}  // Last URL in last item of last group
{{- else if eq $i (sub (len $checks) 1)}}
  {{$urlPrefix = "│   └─"}}  // Last URL in last item of non-last group
{{end}}
```

#### 4. State Tracking

The template maintains several pieces of state:

- `$groupIndex` - Current group number (1-based)
- `$totalGroups` - Total number of groups (calculated once)
- `$i` - Current item index within group (0-based)
- `len $checks` - Number of items in current group

## Template Helper Functions

The template has access to these helper functions (defined in `main.go`):

### Logic Functions
- `yesNo bool` - Converts boolean to "Yes"/"No"
- `not bool` - Boolean negation
- `hasLabel []string string` - Check if slice contains string
- `eq a b` - Equality comparison
- `gt a b` - Greater than comparison

### Arithmetic Functions
- `add a b` - Addition
- `sub a b` - Subtraction
- `len slice` - Length of slice/map

### Data Processing Functions
- `groupChecksByStatus []StatusCheck` - Groups checks by status
- `countGroups map []string` - Counts non-empty groups
- `checkName StatusCheck` - Gets check name (Name field or Context fallback)
- `checkURL StatusCheck` - Gets check URL (DetailsUrl or TargetUrl fallback)
- `ciStatus []StatusCheck` - Summarizes overall CI status

### Formatting Functions
- `slice ...string` - Creates string slice from arguments
- `hyperlink url text` - Creates terminal hyperlink escape sequence
- `supportsHyperlinks` - Detects if terminal supports hyperlinks

### Integration Functions
- `fetchLogs PullRequest StatusCheck` - Fetches error logs from failing checks

## Template Data Structure

Templates receive a `TemplateData` struct with these fields:

```go
type TemplateData struct {
    github.PullRequest        // Embedded PR data
    ShowLogs     bool         // Whether to show error logs (-V flag)
    NoHyperlinks bool         // Whether to disable hyperlinks (--no-hyperlinks)
    PR           github.PullRequest // Duplicate for nested access
}
```

### PR Fields Available

From the embedded `github.PullRequest`:

- `.Number` - PR number
- `.Title` - PR title
- `.AuthorLogin` - Author username
- `.URL` - PR URL
- `.State` - PR state (OPEN, CLOSED, etc.)
- `.CreatedAt` - Creation timestamp
- `.Labels` - Array of label names
- `.StatusCheckRollup.Contexts.Nodes` - Array of CI checks

## Extending Templates

### Adding New Templates

1. Create new `.tmpl` file in this directory
2. Add `//go:embed` directive in `main.go`
3. Create template execution function
4. Wire up to appropriate command-line flags

### Modifying Existing Templates

1. Edit the `.tmpl` file directly
2. Test with real data: `./autoprat -v -r owner/repo`
3. The template is embedded at compile time, so rebuild after changes

### Template Best Practices

1. **Use whitespace control**: `{{-` and `-}}` to avoid extra newlines
2. **Pre-calculate complex logic**: Use variables for expensive operations
3. **Handle edge cases**: Empty lists, missing data, etc.
4. **Test thoroughly**: Templates can be hard to debug at runtime
5. **Document complex logic**: Add comments explaining non-obvious behavior

## Future Template Ideas

- `compact.tmpl` - Single-line PR summaries
- `json.tmpl` - JSON output for machine consumption
- `markdown.tmpl` - Markdown-formatted reports
- `csv.tmpl` - CSV export for spreadsheet analysis
- `html.tmpl` - Rich HTML reports with links and styling

## Debugging Templates

If template execution fails:

1. Check syntax with `go template` tools
2. Verify helper function calls match available functions
3. Add debug output: `{{printf "DEBUG: %+v" .}}`
4. Test with minimal data first
5. Use `--debug` flag to see execution details

The template system provides excellent separation of concerns - data logic in Go, presentation logic in templates.
