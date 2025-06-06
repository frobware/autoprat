// Package main implements a GitHub pull request automation tool for
// filtering PRs and generating commands for PR actions, delegating
// execution to the GitHub CLI (gh).
//
// Features:
//   - Fetches PR data from GitHub GraphQL API
//   - Filters by author, labels, CI status, and PR number
//   - Outputs PRs in tabular or hierarchical formats
//   - Generates `gh` commands for /approve, /lgtm, /ok-to-test, etc.
//   - Commands can be reviewed before execution or piped to shell
//
// By delegating execution to `gh`, this tool:
//   - Avoids reimplementing GitHub API communication
//   - Leverages `gh` authentication and error handling
//   - Enables command inspection before execution
//   - Integrates with shell pipelines
package main

import (
	_ "embed"
	"fmt"
	"log"
	"net/url"
	"os"
	"regexp"
	"slices"
	"sort"
	"strconv"
	"strings"
	"sync"
	"text/tabwriter"
	"text/template"
	"time"

	"github.com/frobware/autoprat/github"
	"github.com/frobware/autoprat/github/actions"
	"github.com/spf13/pflag"
)

//go:embed templates/verbose.tmpl
var verboseTemplate string

// PRArgument represents a parsed PR argument with number and optional repository.
type PRArgument struct {
	Number int
	Repo   string // Empty for numeric arguments, populated for URLs.
}

// parsePRArgument extracts a PR number and repository from either a numeric string or GitHub URL.
// For URLs, extracts both the repository and PR number.
// For numeric arguments, only returns the PR number.
// Supports formats:
//   - "123" (numeric PR number)
//   - "https://github.com/owner/repo/pull/123" (GitHub PR URL)
func parsePRArgument(arg string) (PRArgument, error) {
	// Try parsing as a number first.
	if num, err := strconv.Atoi(arg); err == nil {
		return PRArgument{Number: num}, nil
	}

	// Try parsing as a GitHub URL.
	parsedURL, err := url.Parse(arg)
	if err != nil {
		return PRArgument{}, fmt.Errorf("invalid PR number or URL %q", arg)
	}

	// Match GitHub PR URL pattern: /owner/repo/pull/number.
	re := regexp.MustCompile(`^/([^/]+)/([^/]+)/pull/(\d+)/?$`)
	matches := re.FindStringSubmatch(parsedURL.Path)
	if len(matches) != 4 {
		return PRArgument{}, fmt.Errorf("invalid GitHub PR URL %q", arg)
	}

	urlRepo := matches[1] + "/" + matches[2]
	prNumber, err := strconv.Atoi(matches[3])
	if err != nil {
		return PRArgument{}, fmt.Errorf("invalid PR number in URL %q", arg)
	}

	return PRArgument{Number: prNumber, Repo: urlRepo}, nil
}

// RepositoryPRs holds PRs from a specific repository.
type RepositoryPRs struct {
	Repository string
	PRs        []github.PullRequest
}

var (
	version         = "dev"
	repo            = pflag.StringP("repo", "r", "", "GitHub repo (owner/repo)")
	printGHCommand  = pflag.BoolP("print", "P", false, "Print as gh commands")
	approve         = pflag.Bool("approve", false, "Generate /approve commands for PRs without 'approved' label")
	author          = pflag.StringP("author", "a", "", "Filter by author (exact match)")
	authorSubstring = pflag.StringP("author-substring", "A", "", "Filter by author containing text")
	comment         = pflag.StringSliceP("comment", "c", nil, "Generate comment commands")
	throttle        = pflag.Duration("throttle", 0, "Throttle identical comments to limit posting frequency (e.g. 5m, 1h)")
	debug           = pflag.Bool("debug", false, "Enable debug logging")
	failingCI       = pflag.BoolP("failing-ci", "f", false, "Only show PRs with failing CI")
	failingCheck    = pflag.StringSlice("failing-check", nil, "Only show PRs where specific CI check is failing (exact match, e.g. 'ci/prow/test-fmt')")
	label           = pflag.StringSliceP("label", "l", nil, "Filter by label (prefix with ! to negate)")
	lgtm            = pflag.Bool("lgtm", false, "Generate /lgtm commands for PRs without 'lgtm' label")
	okToTest        = pflag.Bool("ok-to-test", false, "Generate /ok-to-test commands for PRs with needs-ok-to-test label")
	quiet           = pflag.BoolP("quiet", "q", false, "Print PR numbers only")
	verbose         = pflag.BoolP("verbose", "v", false, "Print PR status only")
	verboseVerbose  = pflag.BoolP("verbose-verbose", "V", false, "Print PR status with error logs from failing checks")
	needsApprove    = pflag.Bool("needs-approve", false, "Include only PRs missing the 'approved' label")
	needsLgtm       = pflag.Bool("needs-lgtm", false, "Include only PRs missing the 'lgtm' label")
	needsOkToTest   = pflag.Bool("needs-ok-to-test", false, "Include only PRs that have the 'needs-ok-to-test' label")
	showVersion     = pflag.Bool("version", false, fmt.Sprintf("Show version information (current: %s)", version))
)

func main() {
	pflag.Usage = func() {
		fmt.Fprintf(os.Stderr, "Usage: %s [flags] [PR-NUMBER|PR-URL ...]\n\n", os.Args[0])
		fmt.Fprintf(os.Stderr, `List and filter open GitHub pull requests.

Filter PRs and generate gh(1) commands to apply /lgtm, /approve,
/ok-to-test, and custom comments.

PR arguments can be either numbers (e.g. "123") or GitHub URLs
(e.g. "https://github.com/owner/repo/pull/123").

The --repo flag is required when using numeric PR arguments or when
not providing any PR arguments. When using GitHub URLs, the repository
is extracted from the URL automatically.

`)

		pflag.PrintDefaults()
	}

	pflag.Parse()

	if *showVersion {
		fmt.Println(version)
		os.Exit(0)
	}

	prNumbers := pflag.Args()

	// Parse PR arguments and collect repositories.
	var parsedPRs []PRArgument
	repositories := make(map[string]bool)
	hasNumericArgs := false

	for _, s := range prNumbers {
		prArg, err := parsePRArgument(s)
		if err != nil {
			log.Fatalf("%v", err)
		}
		parsedPRs = append(parsedPRs, prArg)

		if prArg.Repo == "" {
			hasNumericArgs = true
		} else {
			repositories[prArg.Repo] = true
		}
	}

	// Add --repo to the list if specified.
	if *repo != "" {
		repositories[*repo] = true
	}

	// Validate repository requirements.
	if len(repositories) == 0 && (hasNumericArgs || len(prNumbers) == 0) {
		fmt.Fprintf(os.Stderr, "Error: --repo is required when using numeric PR arguments or no PR arguments\n\n")
		pflag.Usage()
		os.Exit(1)
	}

	// Convert repositories map to sorted slice.
	var repoList []string
	for repo := range repositories {
		repoList = append(repoList, repo)
	}
	sort.Strings(repoList)

	if (*approve || *lgtm || *okToTest || len(*comment) > 0) && !*printGHCommand {
		fmt.Fprintf(os.Stderr, "Error: action flags require -P/--print flag\n")
		os.Exit(1)
	}

	// Convert label strings to LabelFilter with negation
	// detection.
	var labels []github.LabelFilter
	for _, rawLabel := range *label {
		negate := false
		labelName := rawLabel
		if strings.HasPrefix(rawLabel, "!") {
			negate = true
			labelName = strings.TrimPrefix(rawLabel, "!")
		}
		labels = append(labels, github.LabelFilter{
			Name:   labelName,
			Negate: negate,
		})
	}

	filter := github.Filter{
		Labels:          labels,
		Author:          *author,
		AuthorSubstring: *authorSubstring,
		OnlyFailingCI:   *failingCI,
		FailingChecks:   *failingCheck,
	}

	// Warn if --ok-to-test is used without --needs-ok-to-test and
	// not printing commands.
	if *okToTest && !*needsOkToTest && !*printGHCommand {
		fmt.Fprintf(os.Stderr, "Hint: --ok-to-test is an action, not a filter. Use --needs-ok-to-test to filter eligible PRs.\n")
	}

	// Warn if action flags are used without -P flag.
	if !*printGHCommand && (*approve || *lgtm || *okToTest || len(*comment) > 0) {
		fmt.Fprintf(os.Stderr, "Warning: Action flags (--approve, --lgtm, --ok-to-test, --comment) require -P/--print to generate gh commands.\n")
	}

	// Fetch PRs from all repositories in parallel.
	var allRepositoryPRs []RepositoryPRs
	var wg sync.WaitGroup
	var mu sync.Mutex
	errChan := make(chan error, len(repoList))

	for _, repository := range repoList {
		wg.Add(1)
		go func(repo string) {
			defer wg.Done()

			client, err := github.NewClient(repo)
			if err != nil {
				errChan <- fmt.Errorf("failed to create client for %s: %v", repo, err)
				return
			}

			prs, err := client.List(filter)
			if err != nil {
				errChan <- fmt.Errorf("failed to list PRs for %s: %v", repo, err)
				return
			}

			mu.Lock()
			allRepositoryPRs = append(allRepositoryPRs, RepositoryPRs{
				Repository: repo,
				PRs:        prs,
			})
			mu.Unlock()
		}(repository)
	}

	wg.Wait()
	close(errChan)

	// Check for errors.
	for err := range errChan {
		log.Fatalf("%v", err)
	}

	// Sort repositories by name for consistent output.
	sort.Slice(allRepositoryPRs, func(i, j int) bool {
		return allRepositoryPRs[i].Repository < allRepositoryPRs[j].Repository
	})

	// Apply global filters to all repositories.
	for i := range allRepositoryPRs {
		prs := allRepositoryPRs[i].PRs

		if *needsApprove {
			prs = filterByLabelAbsence(prs, "approved")
		}
		if *needsLgtm {
			prs = filterByLabelAbsence(prs, "lgtm")
		}
		if *needsOkToTest {
			prs = filterByLabelPresence(prs, "needs-ok-to-test")
		}

		// Apply PR-specific filtering if URLs were provided.
		if len(parsedPRs) > 0 {
			selected := make(map[int]struct{})
			for _, prArg := range parsedPRs {
				if prArg.Repo == "" || prArg.Repo == allRepositoryPRs[i].Repository {
					selected[prArg.Number] = struct{}{}
				}
			}

			if len(selected) > 0 {
				filtered := prs[:0]
				for _, pr := range prs {
					if _, ok := selected[pr.Number]; ok {
						filtered = append(filtered, pr)
					}
				}
				prs = filtered
			}
		}

		allRepositoryPRs[i].PRs = prs
	}

	var allActions []actions.Action

	for _, c := range *comment {
		allActions = append(allActions, actions.Action{
			Comment:   c,
			Predicate: actions.PredicateNone,
		})
	}

	if *okToTest && *printGHCommand {
		allActions = append(allActions, actions.Action{
			Comment:   "/ok-to-test",
			Label:     "needs-ok-to-test",
			Predicate: actions.PredicateOnlyIfLabelExists,
		})
	}

	if *lgtm && *printGHCommand {
		allActions = append(allActions, actions.Action{
			Comment:   "/lgtm",
			Label:     "lgtm",
			Predicate: actions.PredicateSkipIfLabelExists,
		})
	}

	if *approve && *printGHCommand {
		allActions = append(allActions, actions.Action{
			Comment:   "/approve",
			Label:     "approved",
			Predicate: actions.PredicateSkipIfLabelExists,
		})
	}

	if *printGHCommand {
		for _, repoPRs := range allRepositoryPRs {
			for _, prItem := range repoPRs.PRs {
				toPost := actions.FilterActions(allActions, prItem.Labels)
				for _, a := range toPost {
					// Check throttling if specified.
					if *throttle > 0 && github.HasRecentComment(prItem, a.Comment, *throttle) {
						if *debug {
							fmt.Fprintf(os.Stderr, "Skipping comment for PR #%d: recent duplicate found (throttle: %v)\n", prItem.Number, *throttle)
						}
						continue
					}
					fmt.Println(a.Command(repoPRs.Repository, prItem.Number))
				}
			}
		}
		return
	}

	if *verbose || *verboseVerbose {
		for _, repoPRs := range allRepositoryPRs {
			if len(repoPRs.PRs) > 0 {
				fmt.Printf("Repository: %s\n", repoPRs.Repository)
				fmt.Println(strings.Repeat("=", len(repoPRs.Repository)+12))
				for _, pr := range repoPRs.PRs {
					printVerbosePR(pr, *verboseVerbose)
					if *throttle > 0 {
						printThrottleDiagnostics(pr, allActions)
					}
					fmt.Println()
				}
				fmt.Println()
			}
		}
		return
	}

	if *quiet {
		for _, repoPRs := range allRepositoryPRs {
			for _, pr := range repoPRs.PRs {
				fmt.Println(pr.Number)
			}
		}
		return
	}

	tw := tabwriter.NewWriter(os.Stdout, 0, 0, 2, ' ', 0)

	headerRow := "REPOSITORY\tPR URL\tCI\tAPPROVED\tLGTM\tOK2TEST\tHOLD\tAUTHOR\tLAST_COMMENTED\tTITLE"
	fmt.Fprintln(tw, headerRow)

	for _, repoPRs := range allRepositoryPRs {
		for _, pr := range repoPRs.PRs {
			approved := contains(pr.Labels, "approved")
			lgtm := contains(pr.Labels, "lgtm")
			okToTest := contains(pr.Labels, "needs-ok-to-test")
			hold := contains(pr.Labels, "do-not-merge/hold")

			ciStatus := summarizeCIStatus(pr.StatusCheckRollup.Contexts.Nodes)

			lastCommented := getLastCommentTime(pr)
			row := fmt.Sprintf("%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s",
				repoPRs.Repository,
				pr.URL,
				ciStatus,
				yesNo(approved),
				yesNo(lgtm),
				yesNo(!okToTest), // ok-to-test = Y if no 'needs-ok-to-test' label.
				yesNo(hold),
				pr.AuthorLogin,
				lastCommented,
				pr.Title,
			)

			fmt.Fprintln(tw, row)
		}
	}
	tw.Flush()
}

// Template data structure for verbose PR output.
type TemplateData struct {
	github.PullRequest
	ShowLogs bool
	PR       github.PullRequest // For nested access in templates.
}

// Template helper functions.
var templateFuncs = template.FuncMap{
	"yesNo": func(b bool) string {
		if b {
			return "Yes"
		}
		return "No"
	},
	"hasLabel": func(labels []string, target string) bool {
		return slices.Contains(labels, target)
	},
	"not": func(b bool) bool {
		return !b
	},
	"ciStatus": func(checks []github.StatusCheck) string {
		return summarizeCIStatus(checks)
	},
	"sub": func(a, b int) int {
		return a - b
	},
	"add": func(a, b int) int {
		return a + b
	},
	"slice": func(items ...string) []string {
		return items
	},
	"groupChecksByStatus": func(checks []github.StatusCheck) map[string][]github.StatusCheck {
		checksByStatus := make(map[string][]github.StatusCheck)
		for _, check := range checks {
			conclusion := check.Conclusion
			if conclusion == "" {
				conclusion = check.State
			}
			if conclusion == "" {
				conclusion = "UNKNOWN"
			}
			checksByStatus[conclusion] = append(checksByStatus[conclusion], check)
		}
		return checksByStatus
	},
	"countGroups": func(checksByStatus map[string][]github.StatusCheck, statusOrder []string) int {
		totalGroups := 0
		for _, status := range statusOrder {
			if len(checksByStatus[status]) > 0 {
				totalGroups++
			}
		}
		// Add any other statuses not in our predefined order.
		for status := range checksByStatus {
			found := false
			for _, knownStatus := range statusOrder {
				if status == knownStatus {
					found = true
					break
				}
			}
			if !found && len(checksByStatus[status]) > 0 {
				totalGroups++
			}
		}
		return totalGroups
	},
	"checkName": func(check github.StatusCheck) string {
		if check.Name != "" {
			return check.Name
		}
		return check.Context
	},
	"checkURL": func(check github.StatusCheck) string {
		if check.DetailsUrl != "" {
			return check.DetailsUrl
		}
		return check.TargetUrl
	},
	"fetchLogs": func(pr github.PullRequest, check github.StatusCheck) string {
		if logs, err := pr.FetchCheckLogs(check); err == nil && logs != "" {
			return logs
		}
		return ""
	},
}

func printVerbosePR(prItem github.PullRequest, showLogs bool) {
	tmpl, err := template.New("verbose").Funcs(templateFuncs).Parse(verboseTemplate)
	if err != nil {
		log.Printf("Template parse error: %v", err)
		return
	}

	data := TemplateData{
		PullRequest: prItem,
		ShowLogs:    showLogs,
		PR:          prItem,
	}

	if err := tmpl.Execute(os.Stdout, data); err != nil {
		log.Printf("Template execution error: %v", err)
		return
	}
}

func contains(slice []string, item string) bool {
	return slices.Contains(slice, item)
}

func yesNo(b bool) string {
	if b {
		return "Yes"
	}
	return "No"
}

func summarizeCIStatus(checks []github.StatusCheck) string {
	for _, c := range checks {
		st := c.State
		if st == "" {
			st = c.Conclusion
		}
		if st == "FAILURE" {
			return "Failing"
		}
	}
	for _, c := range checks {
		st := c.State
		if st == "" {
			st = c.Conclusion
		}
		if st == "PENDING" {
			return "Pending"
		}
	}
	return "Passing"
}

func filterByLabelAbsence(prs []github.PullRequest, label string) []github.PullRequest {
	filtered := prs[:0]
	for _, pr := range prs {
		if !contains(pr.Labels, label) {
			filtered = append(filtered, pr)
		}
	}
	return filtered
}

func filterByLabelPresence(prs []github.PullRequest, label string) []github.PullRequest {
	filtered := prs[:0]
	for _, pr := range prs {
		if contains(pr.Labels, label) {
			filtered = append(filtered, pr)
		}
	}
	return filtered
}

// printThrottleDiagnostics shows what the throttling logic would do for debugging.
func printThrottleDiagnostics(prItem github.PullRequest, allActions []actions.Action) {
	toPost := actions.FilterActions(allActions, prItem.Labels)
	if len(toPost) == 0 {
		return
	}

	fmt.Printf("├─Throttle Analysis (period: %v)\n", *throttle)

	if len(prItem.Comments) == 0 {
		fmt.Printf("│ └─No recent comments found\n")
		for _, a := range toPost {
			fmt.Printf("│   └─Would post: %s ✓\n", a.Comment)
		}
		return
	}

	fmt.Printf("│ ├─Recent comments (%d found):\n", len(prItem.Comments))

	for i, comment := range prItem.Comments {
		createdAt, err := time.Parse(time.RFC3339, comment.CreatedAt)
		age := ""
		if err == nil {
			age = time.Since(createdAt).Round(time.Minute).String()
		}

		prefix := "│ │ ├─"
		if i == len(prItem.Comments)-1 {
			prefix = "│ │ └─"
		}

		fmt.Printf("%s%s (%s ago): %q\n", prefix, comment.Author.Login, age,
			strings.ReplaceAll(strings.TrimSpace(comment.Body), "\n", " "))
	}

	fmt.Printf("│ └─Action Analysis:\n")
	for i, a := range toPost {
		hasRecent := github.HasRecentComment(prItem, a.Comment, *throttle)
		status := "✓ Would post"
		if hasRecent {
			status = "✗ Throttled (recent duplicate)"
		}

		prefix := "│   ├─"
		if i == len(toPost)-1 {
			prefix = "│   └─"
		}

		fmt.Printf("%s%s: %s\n", prefix, status, a.Comment)
	}
}

// getLastCommentTime returns when any comment was last posted on the PR.
func getLastCommentTime(prItem github.PullRequest) string {
	if len(prItem.Comments) == 0 {
		return "never"
	}

	// Find the most recent comment (any comment).
	var mostRecent time.Time
	found := false

	for _, comment := range prItem.Comments {
		createdAt, err := time.Parse(time.RFC3339, comment.CreatedAt)
		if err != nil {
			continue
		}
		if !found || createdAt.After(mostRecent) {
			mostRecent = createdAt
			found = true
		}
	}

	if !found {
		return "never"
	}

	timeSince := time.Since(mostRecent)
	if timeSince < time.Minute {
		return fmt.Sprintf("%ds", int(timeSince.Seconds()))
	} else if timeSince < time.Hour {
		return fmt.Sprintf("%dm", int(timeSince.Minutes()))
	} else if timeSince < 24*time.Hour {
		return fmt.Sprintf("%dh%dm", int(timeSince.Hours()), int(timeSince.Minutes())%60)
	} else {
		return fmt.Sprintf("%dd", int(timeSince.Hours()/24))
	}
}
