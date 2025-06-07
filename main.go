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
	"os"
	"runtime"
	"runtime/debug"
	"slices"
	"sort"
	"strings"
	"sync"
	"text/tabwriter"
	"text/template"

	"github.com/frobware/autoprat/github"
	"github.com/frobware/autoprat/github/actions"
	"github.com/frobware/autoprat/github/search"
	"github.com/spf13/pflag"
)

//go:embed templates/verbose.tmpl
var verboseTemplate string

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
					if config.Throttle > 0 && github.HasRecentComment(prItem, a.Comment, config.Throttle) {
						if config.DebugMode {
							fmt.Fprintf(os.Stderr, "Skipping comment for PR #%d: recent duplicate found (throttle: %v)\n", prItem.Number, config.Throttle)
						}
						continue
					}
					fmt.Println(a.Command(repoPRs.Repository, prItem.Number))
				}
			}
		}
		return
	}

	if config.Detailed || config.DetailedWithLogs {
		for _, repoPRs := range allRepositoryPRs {
			if len(repoPRs.PRs) > 0 {
				fmt.Printf("Repository: %s\n", repoPRs.Repository)
				fmt.Println(strings.Repeat("=", len(repoPRs.Repository)+12))
				for _, pr := range repoPRs.PRs {
					printDetailedPR(pr, config.DetailedWithLogs)
					if config.Throttle > 0 {
						pr.PrintThrottleDiagnostics(config.Actions, config.Throttle)
					}
					fmt.Println()
				}
				fmt.Println()
			}
		}
		return
	}

	if config.Quiet {
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

			ciStatus := pr.CIStatus()
			lastCommented := pr.LastCommentTime()
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

func main() {
	actionRegistry, err := actions.NewRegistry()
	if err != nil {
		log.Fatalf("Failed to load action registry: %v", err)
	}

	templateRegistry, err := search.NewTemplateRegistry()
	if err != nil {
		log.Fatalf("Failed to load template registry: %v", err)
	}

	// Setup all flags
	flagCategories, flagRefs := SetupFlags(actionRegistry, templateRegistry)

	pflag.Parse()

	// Extract flag values for easier access
	showVersion := flagRefs["version"].(*bool)

	// Build flag maps for parseAndValidateArgs
	actionFlags, templateFlags, parameterisedTemplateFlags := BuildFlagMapsForParsing(flagCategories, flagRefs)

	if *showVersion {
		buildVersion, buildTime, goVer := getBuildInfo()
		fmt.Printf("autoprat version %s\n", buildVersion)
		fmt.Printf("Built: %s\n", buildTime)
		fmt.Printf("Go version: %s\n", goVer)
		fmt.Printf("Platform: %s/%s\n", runtime.GOOS, runtime.GOARCH)
		os.Exit(0)
	}

	config, err := parseAndValidateArgs(actionRegistry, actionFlags, templateRegistry, templateFlags, parameterisedTemplateFlags, flagRefs)
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
	shouldPrintCommands = shouldPrintCommands || len(config.Actions) > 0

	// Always use search API
	allRepositoryPRs, err := fetchAllRepositoryPRsWithSearch(config.Repositories, config.SearchQuery)
	if err != nil {
		log.Fatalf("%v", err)
	}

	// Apply PR-specific filtering (when specific PRs are requested)
	filteredPRs := applyPRFiltering(allRepositoryPRs, config)

	outputResults(filteredPRs, config, shouldPrintCommands)
}

// Template data structure for detailed PR output.
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
		// Create a temporary PullRequest to use the method
		tempPR := github.PullRequest{
			StatusCheckRollup: github.StatusCheckRollup{
				Contexts: struct {
					Nodes []github.StatusCheck `json:"nodes"`
				}{Nodes: checks},
			},
		}
		return tempPR.CIStatus()
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

func printDetailedPR(prItem github.PullRequest, showLogs bool) {
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
