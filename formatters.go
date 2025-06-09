package main

import (
	_ "embed"
	"fmt"
	"os"
	"slices"
	"strings"
	"text/tabwriter"
	"text/template"
)

//go:embed output/templates/verbose.tmpl
var verboseTemplate string

// TemplateData structure for detailed PR output.
type TemplateData struct {
	PullRequest
	ShowLogs bool
	PR       PullRequest // For nested access in templates.
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
	"ciStatus": func(checks []StatusCheck) string {
		// Create a temporary PullRequest to use the method
		tempPR := PullRequest{
			StatusCheckRollup: StatusCheckRollup{
				Contexts: struct {
					Nodes []StatusCheck `json:"nodes"`
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
	"groupChecksByStatus": func(checks []StatusCheck) map[string][]StatusCheck {
		checksByStatus := make(map[string][]StatusCheck)
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
	"countGroups": func(checksByStatus map[string][]StatusCheck, statusOrder []string) int {
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
	"checkName": func(check StatusCheck) string {
		if check.Name != "" {
			return check.Name
		}
		return check.Context
	},
	"checkURL": func(check StatusCheck) string {
		if check.DetailsUrl != "" {
			return check.DetailsUrl
		}
		return check.TargetUrl
	},
	"fetchLogs": func(pr PullRequest, check StatusCheck) string {
		if logs, err := pr.FetchCheckLogs(check); err == nil && logs != "" {
			return logs
		}
		return ""
	},
}

// Formatter defines the interface for different output formats.
type Formatter interface {
	Format(result Result, config *Config) error
}

// TabularFormatter outputs PRs in a table format.
type TabularFormatter struct{}

// VerboseFormatter outputs PRs in detailed verbose format.
type VerboseFormatter struct{}

// QuietFormatter outputs only PR numbers.
type QuietFormatter struct{}

// Format outputs repository PRs in tabular format.
func (f *TabularFormatter) Format(result Result, config *Config) error {
	prResult, ok := result.(PRResult)
	if !ok {
		return fmt.Errorf("TabularFormatter expects PRResult, got %T", result)
	}

	tw := tabwriter.NewWriter(os.Stdout, 0, 0, 2, ' ', 0)
	headerRow := "REPOSITORY\tPR URL\tCI\tAPPROVED\tLGTM\tOK2TEST\tHOLD\tAUTHOR\tLAST_COMMENTED\tTITLE"
	fmt.Fprintln(tw, headerRow)

	for _, repoPRs := range prResult.RepositoryPRs {
		for _, pr := range repoPRs.PRs {
			approved := slices.Contains(pr.Labels, "approved")
			lgtm := slices.Contains(pr.Labels, "lgtm")
			okToTest := slices.Contains(pr.Labels, "needs-ok-to-test")
			hold := slices.Contains(pr.Labels, "do-not-merge/hold")

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
				pr.Author(),
				lastCommented,
				pr.Title,
			)
			fmt.Fprintln(tw, row)
		}
	}
	tw.Flush()
	return nil
}

// Format outputs repository PRs in verbose format.
func (f *VerboseFormatter) Format(result Result, config *Config) error {
	prResult, ok := result.(PRResult)
	if !ok {
		return fmt.Errorf("VerboseFormatter expects PRResult, got %T", result)
	}

	for _, repoPRs := range prResult.RepositoryPRs {
		if len(repoPRs.PRs) > 0 {
			fmt.Printf("Repository: %s\n", repoPRs.Repository)
			fmt.Println(strings.Repeat("=", len(repoPRs.Repository)+12))
			for _, pr := range repoPRs.PRs {
				if err := printDetailedPR(pr, config.DetailedWithLogs); err != nil {
					return fmt.Errorf("failed to print detailed PR: %w", err)
				}
				if config.Throttle > 0 {
					pr.PrintThrottleDiagnostics(config.Actions, config.Throttle)
				}
				fmt.Println()
			}
			fmt.Println()
		}
	}
	return nil
}

// Format outputs only PR numbers.
func (f *QuietFormatter) Format(result Result, config *Config) error {
	prResult, ok := result.(PRResult)
	if !ok {
		return fmt.Errorf("QuietFormatter expects PRResult, got %T", result)
	}

	for _, repoPRs := range prResult.RepositoryPRs {
		for _, pr := range repoPRs.PRs {
			fmt.Println(pr.Number)
		}
	}
	return nil
}

// printDetailedPR renders a PR using the verbose template.
func printDetailedPR(prItem PullRequest, showLogs bool) error {
	tmpl, err := template.New("verbose").Funcs(templateFuncs).Parse(verboseTemplate)
	if err != nil {
		return fmt.Errorf("template parse error: %w", err)
	}

	data := TemplateData{
		PullRequest: prItem,
		ShowLogs:    showLogs,
		PR:          prItem,
	}

	if err := tmpl.Execute(os.Stdout, data); err != nil {
		return fmt.Errorf("template execution error: %w", err)
	}
	return nil
}

// Helper functions.

func yesNo(b bool) string {
	if b {
		return "Yes"
	}
	return "No"
}
