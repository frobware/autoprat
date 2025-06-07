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
	"path/filepath"
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
	"github.com/frobware/autoprat/github/filters"
	"github.com/frobware/autoprat/github/search"
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
	if num, err := strconv.Atoi(arg); err == nil {
		return PRArgument{Number: num}, nil
	}

	parsedURL, err := url.Parse(arg)
	if err != nil {
		return PRArgument{}, fmt.Errorf("invalid PR number or URL %q", arg)
	}

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
		if info.Main.Version != "(devel)" && info.Main.Version != "" {
			buildVersion = info.Main.Version
		}

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
	Actions      []actions.Action
	Filters      []filters.FilterDefinition
	SearchQuery  string
}

// parseAndValidateArgs parses command line arguments and validates
// repository requirements.
func parseAndValidateArgs(actionRegistry *actions.Registry, actionFlags map[string]*bool, templateRegistry *search.TemplateRegistry, templateFlags map[string]*bool, parameterizedTemplateFlags map[string]interface{}) (*Config, error) {
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

	var allActions []actions.Action
	for _, c := range *comment {
		allActions = append(allActions, actions.Action{
			Comment:   c,
			Predicate: actions.PredicateNone,
		})
	}

	for flag, flagPtr := range actionFlags {
		if *flagPtr {
			actionDef, exists := actionRegistry.GetAction(flag)
			if exists {
				allActions = append(allActions, actionDef.ToAction())
			}
		}
	}

	// Build search query from templates
	var queryTerms []string

	// Handle boolean templates (non-parameterized)
	for flag, flagPtr := range templateFlags {
		if *flagPtr {
			template, exists := templateRegistry.GetTemplate(flag)
			if exists && !template.Parameterized {
				queryTerms = append(queryTerms, template.Query)
			}
		}
	}

	// Handle parameterized templates
	for flag, flagPtr := range parameterizedTemplateFlags {
		_, exists := templateRegistry.GetTemplate(flag)
		if !exists {
			continue
		}

		var query string
		var queryErr error

		if stringPtr, ok := flagPtr.(*string); ok && *stringPtr != "" {
			query, queryErr = templateRegistry.BuildQuery(flag, *stringPtr, nil)
		} else if slicePtr, ok := flagPtr.(*[]string); ok && len(*slicePtr) > 0 {
			query, queryErr = templateRegistry.BuildQuery(flag, "", *slicePtr)
		}

		if queryErr == nil && query != "" {
			queryTerms = append(queryTerms, query)
		}
	}

	searchQuery := strings.Join(queryTerms, " ")

	return &Config{
		Repositories: repoList,
		ParsedPRs:    parsedPRs,
		Actions:      allActions,
		SearchQuery:  searchQuery,
	}, nil
}

// fetchAllRepositoryPRsWithSearch fetches PRs from all repositories using the search API.
func fetchAllRepositoryPRsWithSearch(repositories []string, searchQuery string) ([]RepositoryPRs, error) {
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

			prs, err := client.Search(searchQuery)
			if err != nil {
				errChan <- fmt.Errorf("failed to search PRs for %s: %v", repo, err)
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
func printGroupedFlags(actionRegistry *actions.Registry, templateRegistry *search.TemplateRegistry) {
	fmt.Fprintf(os.Stderr, "Repository:\n")
	fmt.Fprintf(os.Stderr, "  -r, --repo string               GitHub repo (owner/repo)\n\n")

	fmt.Fprintf(os.Stderr, "Filters:\n")

	// Show built-in templates
	builtinTemplateFlags := templateRegistry.GetFlagsBySource("embedded")
	for _, flag := range builtinTemplateFlags {
		template, _ := templateRegistry.GetTemplate(flag)
		flagDisplay := fmt.Sprintf("--%s", flag)
		if template.FlagShort != "" {
			flagDisplay = fmt.Sprintf("-%s, --%s", template.FlagShort, flag)
		}

		// Add parameter type for parameterized templates
		if template.Parameterized {
			if template.SupportsMultiple {
				flagDisplay += " strings"
			} else {
				flagDisplay += " string"
			}
		}

		fmt.Fprintf(os.Stderr, "  %-31s %s\n", flagDisplay, template.Description)
	}

	// Show user-defined templates if any exist
	userTemplateFlags := templateRegistry.GetFlagsBySource("user")
	if len(userTemplateFlags) > 0 {
		homeDir, _ := os.UserHomeDir()
		templatesPath := filepath.Join(homeDir, ".config", "autoprat", "templates")
		fmt.Fprintf(os.Stderr, "\n  User-defined filters (from %s):\n", templatesPath)
		for _, flag := range userTemplateFlags {
			template, _ := templateRegistry.GetTemplate(flag)
			flagDisplay := fmt.Sprintf("--%s", flag)
			if template.FlagShort != "" {
				flagDisplay = fmt.Sprintf("-%s, --%s", template.FlagShort, flag)
			}

			// Add parameter type for parameterized templates
			if template.Parameterized {
				if template.SupportsMultiple {
					flagDisplay += " strings"
				} else {
					flagDisplay += " string"
				}
			}

			fmt.Fprintf(os.Stderr, "  %-31s %s\n", flagDisplay, template.Description)
		}
	}

	fmt.Fprintf(os.Stderr, "\n")

	fmt.Fprintf(os.Stderr, "Actions:\n")
	builtinFlags := actionRegistry.GetFlagsBySource("embedded")
	for _, flag := range builtinFlags {
		action, _ := actionRegistry.GetAction(flag)
		fmt.Fprintf(os.Stderr, "      --%-25s %s\n", flag, action.Description)
	}

	userFlags := actionRegistry.GetFlagsBySource("user")
	if len(userFlags) > 0 {
		homeDir, _ := os.UserHomeDir()
		actionsPath := filepath.Join(homeDir, ".config", "autoprat", "actions")
		fmt.Fprintf(os.Stderr, "\n  User-defined actions (from %s):\n", actionsPath)
		for _, flag := range userFlags {
			action, _ := actionRegistry.GetAction(flag)
			fmt.Fprintf(os.Stderr, "      --%-25s %s\n", flag, action.Description)
		}
	}

	fmt.Fprintf(os.Stderr, "\n  -c, --comment strings           Generate comment commands\n")
	fmt.Fprintf(os.Stderr, "      --throttle duration         Throttle identical comments to limit posting frequency\n\n")

	fmt.Fprintf(os.Stderr, "Output:\n")
	fmt.Fprintf(os.Stderr, "  -v, --verbose                   Print PR status only\n")
	fmt.Fprintf(os.Stderr, "  -V, --verbose-verbose           Print PR status with error logs from failing checks\n")
	fmt.Fprintf(os.Stderr, "  -q, --quiet                     Print PR numbers only\n\n")

	fmt.Fprintf(os.Stderr, "Other:\n")
	fmt.Fprintf(os.Stderr, "      --debug                     Enable debug logging\n")
	fmt.Fprintf(os.Stderr, "      --version                   Show version information\n")
}

// applyPRFiltering applies PR-specific filtering when specific PRs are requested.
func applyPRFiltering(allRepositoryPRs []RepositoryPRs, config *Config) []RepositoryPRs {
	for i := range allRepositoryPRs {
		prs := allRepositoryPRs[i].PRs

		if len(config.ParsedPRs) > 0 {
			selected := make(map[int]struct{})
			for _, prArg := range config.ParsedPRs {
				if prArg.Repo == "" || prArg.Repo == allRepositoryPRs[i].Repository {
					selected[prArg.Number] = struct{}{}
				}
			}

			if len(selected) > 0 {
				var filtered []github.PullRequest
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
	// Static flags that don't come from registries
	repo           = pflag.StringP("repo", "r", "", "GitHub repo (owner/repo)")
	comment        = pflag.StringSliceP("comment", "c", nil, "Generate comment commands")
	throttle       = pflag.Duration("throttle", 0, "Throttle identical comments to limit posting frequency (e.g. 5m, 1h)")
	debugMode      = pflag.Bool("debug", false, "Enable debug logging")
	quiet          = pflag.BoolP("quiet", "q", false, "Print PR numbers only")
	verbose        = pflag.BoolP("verbose", "v", false, "Print PR status only")
	verboseVerbose = pflag.BoolP("verbose-verbose", "V", false, "Print PR status with error logs from failing checks")
	showVersion    = pflag.Bool("version", false, "Show version information")
)

func main() {
	actionRegistry, err := actions.NewRegistry()
	if err != nil {
		log.Fatalf("Failed to load action registry: %v", err)
	}

	templateRegistry, err := search.NewTemplateRegistry()
	if err != nil {
		log.Fatalf("Failed to load template registry: %v", err)
	}

	actionFlags := make(map[string]*bool)
	for flag, action := range actionRegistry.GetAllActions() {
		actionFlags[flag] = pflag.Bool(flag, false, action.Description)
	}

	templateFlags := make(map[string]*bool)
	parameterizedTemplateFlags := make(map[string]interface{})

	for flag, template := range templateRegistry.GetAllTemplates() {
		if template.Parameterized {
			// Register parameterized templates
			if template.SupportsMultiple {
				if template.FlagShort != "" {
					parameterizedTemplateFlags[flag] = pflag.StringSliceP(flag, template.FlagShort, nil, template.Description)
				} else {
					parameterizedTemplateFlags[flag] = pflag.StringSlice(flag, nil, template.Description)
				}
			} else {
				if template.FlagShort != "" {
					parameterizedTemplateFlags[flag] = pflag.StringP(flag, template.FlagShort, "", template.Description)
				} else {
					parameterizedTemplateFlags[flag] = pflag.String(flag, "", template.Description)
				}
			}
		} else {
			// Register boolean templates
			if template.FlagShort != "" {
				templateFlags[flag] = pflag.BoolP(flag, template.FlagShort, false, template.Description)
			} else {
				templateFlags[flag] = pflag.Bool(flag, false, template.Description)
			}
		}
	}

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

		printGroupedFlags(actionRegistry, templateRegistry)

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

	config, err := parseAndValidateArgs(actionRegistry, actionFlags, templateRegistry, templateFlags, parameterizedTemplateFlags)
	if err != nil {
		pflag.Usage()
		fmt.Fprintf(os.Stderr, "\nError: %v\n", err)
		os.Exit(1)
	}

	shouldPrintCommands := false
	for _, flagPtr := range actionFlags {
		if *flagPtr {
			shouldPrintCommands = true
			break
		}
	}
	shouldPrintCommands = shouldPrintCommands || len(*comment) > 0

	// Always use search API
	allRepositoryPRs, err := fetchAllRepositoryPRsWithSearch(config.Repositories, config.SearchQuery)
	if err != nil {
		log.Fatalf("%v", err)
	}

	// Apply PR-specific filtering (when specific PRs are requested)
	filteredPRs := applyPRFiltering(allRepositoryPRs, config)

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
