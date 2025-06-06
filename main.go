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
	"runtime"
	"runtime/debug"
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

// getBuildInfo returns version information from runtime build info.
func getBuildInfo() (string, string, string) {
	buildVersion := "unknown"
	buildTime := "unknown"
	goVer := runtime.Version()

	if info, ok := debug.ReadBuildInfo(); ok {
		// Use module version if available.
		if info.Main.Version != "(devel)" && info.Main.Version != "" {
			buildVersion = info.Main.Version
		}

		// Look for VCS information.
		for _, setting := range info.Settings {
			switch setting.Key {
			case "vcs.time":
				buildTime = setting.Value
			}
		}
	}

	return buildVersion, buildTime, goVer
}

// RepositoryPRs holds PRs from a specific repository.
type RepositoryPRs struct {
	Repository string
	PRs        []github.PullRequest
}

// Config holds all configuration and arguments for the application.
type Config struct {
	Repositories []string
	ParsedPRs    []PRArgument
	Filter       github.Filter
	Actions      []actions.Action
}

// parseAndValidateArgs parses command line arguments and validates
// repository requirements.
func parseAndValidateArgs() (*Config, error) {
	prNumbers := pflag.Args()

	var parsedPRs []PRArgument
	repositories := make(map[string]bool)
	hasNumericArgs := false

	for _, s := range prNumbers {
		prArg, err := parsePRArgument(s)
		if err != nil {
			return nil, err
		}
		parsedPRs = append(parsedPRs, prArg)

		if prArg.Repo == "" {
			hasNumericArgs = true
		} else {
			repositories[prArg.Repo] = true
		}
	}

	if *repo != "" {
		repositories[*repo] = true
	}

	if len(repositories) == 0 && (hasNumericArgs || len(prNumbers) == 0) {
		return nil, fmt.Errorf("--repo is required when using numeric PR arguments or no PR arguments")
	}

	var repoList []string
	for repo := range repositories {
		repoList = append(repoList, repo)
	}
	sort.Strings(repoList)

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

	var allActions []actions.Action
	for _, c := range *comment {
		allActions = append(allActions, actions.Action{
			Comment:   c,
			Predicate: actions.PredicateNone,
		})
	}

	if *okToTest {
		allActions = append(allActions, actions.Action{
			Comment:   "/ok-to-test",
			Label:     "needs-ok-to-test",
			Predicate: actions.PredicateOnlyIfLabelExists,
		})
	}

	if *lgtm {
		allActions = append(allActions, actions.Action{
			Comment:   "/lgtm",
			Label:     "lgtm",
			Predicate: actions.PredicateSkipIfLabelExists,
		})
	}

	if *approve {
		allActions = append(allActions, actions.Action{
			Comment:   "/approve",
			Label:     "approved",
			Predicate: actions.PredicateSkipIfLabelExists,
		})
	}

	return &Config{
		Repositories: repoList,
		ParsedPRs:    parsedPRs,
		Filter:       filter,
		Actions:      allActions,
	}, nil
}

// fetchAllRepositoryPRs fetches PRs from all repositories in parallel.
func fetchAllRepositoryPRs(repositories []string, filter github.Filter) ([]RepositoryPRs, error) {
	var allRepositoryPRs []RepositoryPRs
	var wg sync.WaitGroup
	var mu sync.Mutex
	errChan := make(chan error, len(repositories))

	for _, repository := range repositories {
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

	for err := range errChan {
		return nil, err
	}

	sort.Slice(allRepositoryPRs, func(i, j int) bool {
		return allRepositoryPRs[i].Repository < allRepositoryPRs[j].Repository
	})

	return allRepositoryPRs, nil
}

// printGroupedFlags prints command line flags organized into logical groups.
func printGroupedFlags() {
	fmt.Fprintf(os.Stderr, "Repository:\n")
	fmt.Fprintf(os.Stderr, "  -r, --repo string               GitHub repo (owner/repo)\n\n")

	fmt.Fprintf(os.Stderr, "Filters:\n")
	fmt.Fprintf(os.Stderr, "  -a, --author string             Filter by author (exact match)\n")
	fmt.Fprintf(os.Stderr, "  -A, --author-substring string   Filter by author containing text\n")
	fmt.Fprintf(os.Stderr, "  -l, --label strings             Filter by label (prefix with ! to negate)\n")
	fmt.Fprintf(os.Stderr, "  -f, --failing-ci                Only show PRs with failing CI\n")
	fmt.Fprintf(os.Stderr, "      --failing-check strings     Only show PRs where specific CI check is failing (exact match)\n")
	fmt.Fprintf(os.Stderr, "      --needs-approve             Include only PRs missing the 'approved' label\n")
	fmt.Fprintf(os.Stderr, "      --needs-lgtm                Include only PRs missing the 'lgtm' label\n")
	fmt.Fprintf(os.Stderr, "      --needs-ok-to-test          Include only PRs that have the 'needs-ok-to-test' label\n\n")

	fmt.Fprintf(os.Stderr, "Actions:\n")
	fmt.Fprintf(os.Stderr, "      --approve                   Generate /approve commands for PRs without 'approved' label\n")
	fmt.Fprintf(os.Stderr, "      --lgtm                      Generate /lgtm commands for PRs without 'lgtm' label\n")
	fmt.Fprintf(os.Stderr, "      --ok-to-test                Generate /ok-to-test commands for PRs with needs-ok-to-test label\n")
	fmt.Fprintf(os.Stderr, "  -c, --comment strings           Generate comment commands\n")
	fmt.Fprintf(os.Stderr, "      --throttle duration         Throttle identical comments to limit posting frequency\n\n")

	fmt.Fprintf(os.Stderr, "Output:\n")
	fmt.Fprintf(os.Stderr, "  -v, --verbose                   Print PR status only\n")
	fmt.Fprintf(os.Stderr, "  -V, --verbose-verbose           Print PR status with error logs from failing checks\n")
	fmt.Fprintf(os.Stderr, "  -q, --quiet                     Print PR numbers only\n\n")

	fmt.Fprintf(os.Stderr, "Other:\n")
	fmt.Fprintf(os.Stderr, "      --debug                     Enable debug logging\n")
	fmt.Fprintf(os.Stderr, "      --version                   Show version information\n")
}

// applyFilters applies global filters and PR-specific filtering to all repositories.
func applyFilters(allRepositoryPRs []RepositoryPRs, config *Config) []RepositoryPRs {
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

		if len(config.ParsedPRs) > 0 {
			selected := make(map[int]struct{})
			for _, prArg := range config.ParsedPRs {
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

	return allRepositoryPRs
}

// outputResults handles all output formats based on command line flags.
func outputResults(allRepositoryPRs []RepositoryPRs, config *Config, shouldPrintCommands bool) {
	if shouldPrintCommands {
		for _, repoPRs := range allRepositoryPRs {
			for _, prItem := range repoPRs.PRs {
				toPost := actions.FilterActions(config.Actions, prItem.Labels)
				for _, a := range toPost {
					if *throttle > 0 && github.HasRecentComment(prItem, a.Comment, *throttle) {
						if *debugMode {
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
						printThrottleDiagnostics(pr, config.Actions)
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
				yesNo(!okToTest),
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

var (
	repo            = pflag.StringP("repo", "r", "", "GitHub repo (owner/repo)")
	approve         = pflag.Bool("approve", false, "Generate /approve commands for PRs without 'approved' label")
	author          = pflag.StringP("author", "a", "", "Filter by author (exact match)")
	authorSubstring = pflag.StringP("author-substring", "A", "", "Filter by author containing text")
	comment         = pflag.StringSliceP("comment", "c", nil, "Generate comment commands")
	throttle        = pflag.Duration("throttle", 0, "Throttle identical comments to limit posting frequency (e.g. 5m, 1h)")
	debugMode       = pflag.Bool("debug", false, "Enable debug logging")
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
	showVersion     = pflag.Bool("version", false, "Show version information")
)

func main() {
	pflag.Usage = func() {
		fmt.Fprintf(os.Stderr, "Usage: %s [flags] [PR-NUMBER|PR-URL ...]\n\n", os.Args[0])
		fmt.Fprintf(os.Stderr, `List and filter open GitHub pull requests.

Filter PRs and generate gh(1) commands to apply /lgtm, /approve,
/ok-to-test, and custom comments.

By default, lists all open PRs when --repo is specified alone.
Optionally focus on specific PRs by providing:
  - PR numbers (e.g. "123", requires --repo)
  - GitHub URLs (e.g. "https://github.com/owner/repo/pull/123")

When using GitHub URLs, the repository is extracted from the URL
automatically and --repo is not required.

`)

		printGroupedFlags()

		fmt.Fprintf(os.Stderr, `
Examples:
  # List all open PRs in a repository.
  %[1]s -r owner/repo

  # Filter PRs that need approval.
  %[1]s -r owner/repo --needs-approve

  # Generate approval commands for Dependabot PRs.
  %[1]s -r owner/repo --author dependabot --approve

  # Execute the generated commands.
  %[1]s -r owner/repo --author dependabot --approve | sh

  # Focus on specific PRs.
  %[1]s -r owner/repo --verbose 123 456
  %[1]s --verbose https://github.com/owner/repo/pull/123
`, os.Args[0])
	}

	pflag.Parse()

	if *showVersion {
		buildVersion, buildTime, goVer := getBuildInfo()
		fmt.Printf("autoprat version %s\n", buildVersion)
		fmt.Printf("Built: %s\n", buildTime)
		fmt.Printf("Go version: %s\n", goVer)
		fmt.Printf("Platform: %s/%s\n", runtime.GOOS, runtime.GOARCH)
		os.Exit(0)
	}

	config, err := parseAndValidateArgs()
	if err != nil {
		pflag.Usage()
		fmt.Fprintf(os.Stderr, "\nError: %v\n", err)
		os.Exit(1)
	}

	// Print gh commands when action flags are used.
	shouldPrintCommands := (*approve || *lgtm || *okToTest || len(*comment) > 0)

	if *okToTest && !*needsOkToTest {
		fmt.Fprintf(os.Stderr, "Hint: --ok-to-test is an action, not a filter. Use --needs-ok-to-test to filter eligible PRs.\n")
	}

	allRepositoryPRs, err := fetchAllRepositoryPRs(config.Repositories, config.Filter)
	if err != nil {
		log.Fatalf("%v", err)
	}

	filteredPRs := applyFilters(allRepositoryPRs, config)

	outputResults(filteredPRs, config, shouldPrintCommands)
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

// printThrottleDiagnostics shows what the throttling logic would do
// for debugging.
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

// getLastCommentTime returns when any comment was last posted on the
// PR.
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
