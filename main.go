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
	"fmt"
	"log"
	"os"
	"slices"
	"strings"
	"text/tabwriter"
	"time"

	"github.com/frobware/autoprat/pr"
	"github.com/frobware/autoprat/pr/actions"
	"github.com/spf13/pflag"
	"golang.org/x/term"
)

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
	noHyperlinks    = pflag.Bool("no-hyperlinks", false, "Disable terminal hyperlinks, show URLs explicitly")
	needsApprove    = pflag.Bool("needs-approve", false, "Include only PRs missing the 'approved' label")
	needsLgtm       = pflag.Bool("needs-lgtm", false, "Include only PRs missing the 'lgtm' label")
	needsOkToTest   = pflag.Bool("needs-ok-to-test", false, "Include only PRs that have the 'needs-ok-to-test' label")
	showVersion     = pflag.Bool("version", false, fmt.Sprintf("Show version information (current: %s)", version))
)

func main() {
	pflag.Usage = func() {
		fmt.Fprintf(os.Stderr, "Usage: %s [flags] [PR-NUMBER ...]\n\n", os.Args[0])
		fmt.Fprintf(os.Stderr, `List and filter open GitHub pull requests.

Filter PRs and generate gh(1) commands to apply /lgtm, /approve,
/ok-to-test, and custom comments.

`)

		pflag.PrintDefaults()
	}

	pflag.Parse()

	if *showVersion {
		fmt.Println(version)
		os.Exit(0)
	}

	if *repo == "" {
		fmt.Fprintf(os.Stderr, "Error: --repo is required\n\n")
		pflag.Usage()
		os.Exit(1)
	}

	if (*approve || *lgtm || *okToTest || len(*comment) > 0) && !*printGHCommand {
		fmt.Fprintf(os.Stderr, "Error: action flags require -P/--print flag\n")
		os.Exit(1)
	}

	prNumbers := pflag.Args()

	// Convert label strings to LabelFilter with negation
	// detection.
	var labels []pr.LabelFilter
	for _, rawLabel := range *label {
		negate := false
		labelName := rawLabel
		if strings.HasPrefix(rawLabel, "!") {
			negate = true
			labelName = strings.TrimPrefix(rawLabel, "!")
		}
		labels = append(labels, pr.LabelFilter{
			Name:   labelName,
			Negate: negate,
		})
	}

	filter := pr.Filter{
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

	client, err := pr.NewClient(*repo)
	if err != nil {
		log.Fatalf("failed to create client: %v", err)
	}

	prs, err := client.List(filter)
	if err != nil {
		log.Fatalf("failed to list PRs: %v", err)
	}

	if *needsApprove {
		prs = filterByLabelAbsence(prs, "approved")
	}
	if *needsLgtm {
		prs = filterByLabelAbsence(prs, "lgtm")
	}
	if *needsOkToTest {
		prs = filterByLabelPresence(prs, "needs-ok-to-test")
	}

	if len(prNumbers) > 0 {
		selected := make(map[int]struct{}, len(prNumbers))
		for _, s := range prNumbers {
			var num int
			if _, err := fmt.Sscanf(s, "%d", &num); err != nil {
				log.Fatalf("invalid PR number %q: %v", s, err)
			}
			selected[num] = struct{}{}
		}

		filtered := prs[:0]
		for _, pr := range prs {
			if _, ok := selected[pr.Number]; ok {
				filtered = append(filtered, pr)
			}
		}
		prs = filtered
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
		for _, prItem := range prs {
			toPost := actions.FilterActions(allActions, prItem.Labels)
			for _, a := range toPost {
				// Check throttling if specified
				if *throttle > 0 && pr.HasRecentComment(prItem, a.Comment, *throttle) {
					if *debug {
						fmt.Fprintf(os.Stderr, "Skipping comment for PR #%d: recent duplicate found (throttle: %v)\n", prItem.Number, *throttle)
					}
					continue
				}
				fmt.Println(a.Command(*repo, prItem.Number))
			}
		}
		return
	}

	if *verbose || *verboseVerbose {
		for _, pr := range prs {
			printVerbosePR(pr, *verboseVerbose, *noHyperlinks)
			if *throttle > 0 {
				printThrottleDiagnostics(pr, allActions)
			}
			fmt.Println()
		}
		return
	}

	if *quiet {
		for _, pr := range prs {
			fmt.Println(pr.Number)
		}
		return
	}

	tw := tabwriter.NewWriter(os.Stdout, 0, 0, 2, ' ', 0)

	headerRow := "PR URL\tCI\tAPPROVED\tLGTM\tOK2TEST\tHOLD\tAUTHOR\tLAST_COMMENTED\tTITLE"
	fmt.Fprintln(tw, headerRow)

	for _, pr := range prs {
		approved := contains(pr.Labels, "approved")
		lgtm := contains(pr.Labels, "lgtm")
		okToTest := contains(pr.Labels, "needs-ok-to-test")
		hold := contains(pr.Labels, "do-not-merge/hold")

		ciStatus := summarizeCIStatus(pr.StatusCheckRollup.Contexts.Nodes)

		lastCommented := getLastCommentTime(pr)
		row := fmt.Sprintf("%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s",
			pr.URL,
			ciStatus,
			yesNo(approved),
			yesNo(lgtm),
			yesNo(!okToTest), // ok-to-test = Y if no 'needs-ok-to-test' label
			yesNo(hold),
			pr.AuthorLogin,
			lastCommented,
			pr.Title,
		)

		fmt.Fprintln(tw, row)
	}
	tw.Flush()
}

func printVerbosePR(prItem pr.PullRequest, showLogs bool, noHyperlinks bool) {
	fmt.Printf("● %s\n", prItem.URL)
	fmt.Printf("├─Title: %s (%s)\n", prItem.Title, prItem.AuthorLogin)
	fmt.Printf("├─PR #%d\n", prItem.Number)
	fmt.Printf("├─State: %s\n", prItem.State)
	fmt.Printf("├─Created: %s\n", prItem.CreatedAt)
	fmt.Printf("├─Status\n")

	approved := contains(prItem.Labels, "approved")
	lgtm := contains(prItem.Labels, "lgtm")
	okToTest := contains(prItem.Labels, "needs-ok-to-test")

	fmt.Printf("│ ├─Approved: %s\n", yesNo(approved))
	fmt.Printf("│ ├─CI: %s\n", summarizeCIStatus(prItem.StatusCheckRollup.Contexts.Nodes))
	fmt.Printf("│ ├─LGTM: %s\n", yesNo(lgtm))
	fmt.Printf("│ └─OK-to-test: %s\n", yesNo(!okToTest))

	fmt.Printf("├─Labels\n")
	if len(prItem.Labels) == 0 {
		fmt.Printf("│ └─None\n")
	} else {
		for i, label := range prItem.Labels {
			prefix := "│ ├─"
			if i == len(prItem.Labels)-1 {
				prefix = "│ └─"
			}
			fmt.Printf("%s%s\n", prefix, label)
		}
	}

	fmt.Printf("└─Checks\n")
	checks := prItem.StatusCheckRollup.Contexts.Nodes
	if len(checks) == 0 {
		fmt.Println("  └─None")
	} else {
		// Group checks by status
		checksByStatus := make(map[string][]pr.StatusCheck)
		statusOrder := []string{"FAILURE", "PENDING", "SUCCESS"}

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

		groupIndex := 0
		totalGroups := 0
		for _, status := range statusOrder {
			if len(checksByStatus[status]) > 0 {
				totalGroups++
			}
		}
		// Add any other statuses not in our predefined order
		for status := range checksByStatus {
			found := false
			for _, knownStatus := range statusOrder {
				if status == knownStatus {
					found = true
					break
				}
			}
			if !found {
				totalGroups++
			}
		}

		for _, status := range statusOrder {
			if len(checksByStatus[status]) == 0 {
				continue
			}

			groupIndex++
			groupPrefix := "├─"
			if groupIndex == totalGroups {
				groupPrefix = "└─"
			}

			fmt.Printf("  %s%s (%d)\n", groupPrefix, status, len(checksByStatus[status]))

			for i, check := range checksByStatus[status] {
				itemPrefix := "│ ├─"
				if groupIndex == totalGroups && i == len(checksByStatus[status])-1 {
					itemPrefix = "│ └─"
				} else if i == len(checksByStatus[status])-1 {
					itemPrefix = "│ └─"
				}

				name := check.Name
				if name == "" {
					name = check.Context
				}

				url := check.DetailsUrl
				if url == "" {
					url = check.TargetUrl
				}

				supportsHyperlinks := !noHyperlinks && term.IsTerminal(int(os.Stdout.Fd())) && terminalSupportsHyperlinks()

				if url != "" && supportsHyperlinks {
					nameText := fmt.Sprintf("\033]8;;%s\033\\%s\033]8;;\033\\", url, name)
					fmt.Printf("  %s%s\n", itemPrefix, nameText)
				} else {
					fmt.Printf("  %s%s\n", itemPrefix, name)
					if url != "" {
						urlPrefix := "│ │   └─"
						if groupIndex == totalGroups && i == len(checksByStatus[status])-1 {
							urlPrefix = "    └─"
						} else if i == len(checksByStatus[status])-1 {
							urlPrefix = "│   └─"
						}
						fmt.Printf("  %sURL: %s\n", urlPrefix, url)
					}
				}

				if showLogs && status == "FAILURE" {
					if logs, err := prItem.FetchCheckLogs(check); err == nil && logs != "" {
						fmt.Printf("    │ Error logs:\n%s\n", logs)
					} else if err != nil {
						fmt.Printf("    │ (Could not fetch logs: %v)\n", err)
					}
				}
			}
		}

		// Handle any other statuses not in our predefined order
		for status := range checksByStatus {
			found := false
			for _, knownStatus := range statusOrder {
				if status == knownStatus {
					found = true
					break
				}
			}
			if !found && len(checksByStatus[status]) > 0 {
				groupIndex++
				groupPrefix := "├─"
				if groupIndex == totalGroups {
					groupPrefix = "└─"
				}

				fmt.Printf("  %s%s (%d)\n", groupPrefix, status, len(checksByStatus[status]))

				for i, check := range checksByStatus[status] {
					itemPrefix := "│ ├─"
					if groupIndex == totalGroups {
						// This is the last group
						if i == len(checksByStatus[status])-1 {
							itemPrefix = "  └─"
						} else {
							itemPrefix = "  ├─"
						}
					} else if i == len(checksByStatus[status])-1 {
						itemPrefix = "│ └─"
					}

					name := check.Name
					if name == "" {
						name = check.Context
					}

					url := check.DetailsUrl
					if url == "" {
						url = check.TargetUrl
					}

					supportsHyperlinks := !noHyperlinks && term.IsTerminal(int(os.Stdout.Fd())) && terminalSupportsHyperlinks()

					if url != "" && supportsHyperlinks {
						nameText := fmt.Sprintf("\033]8;;%s\033\\%s\033]8;;\033\\", url, name)
						fmt.Printf("  %s%s\n", itemPrefix, nameText)
					} else {
						fmt.Printf("  %s%s\n", itemPrefix, name)
						if url != "" {
							urlPrefix := "│ │   └─"
							if groupIndex == totalGroups {
								if i == len(checksByStatus[status])-1 {
									urlPrefix = "    └─"
								} else {
									urlPrefix = "  │   └─"
								}
							} else if i == len(checksByStatus[status])-1 {
								urlPrefix = "│   └─"
							}
							fmt.Printf("  %sURL: %s\n", urlPrefix, url)
						}
					}

					if showLogs && status == "FAILURE" {
						if logs, err := prItem.FetchCheckLogs(check); err == nil && logs != "" {
							fmt.Printf("    │ Error logs:\n%s\n", logs)
						} else if err != nil {
							fmt.Printf("    │ (Could not fetch logs: %v)\n", err)
						}
					}
				}
			}
		}
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

func summarizeCIStatus(checks []pr.StatusCheck) string {
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

func filterByLabelAbsence(prs []pr.PullRequest, label string) []pr.PullRequest {
	filtered := prs[:0]
	for _, pr := range prs {
		if !contains(pr.Labels, label) {
			filtered = append(filtered, pr)
		}
	}
	return filtered
}

func filterByLabelPresence(prs []pr.PullRequest, label string) []pr.PullRequest {
	filtered := prs[:0]
	for _, pr := range prs {
		if contains(pr.Labels, label) {
			filtered = append(filtered, pr)
		}
	}
	return filtered
}

// terminalSupportsHyperlinks detects if the terminal supports OSC 8 hyperlinks
func terminalSupportsHyperlinks() bool {
	term := os.Getenv("TERM")
	termProgram := os.Getenv("TERM_PROGRAM")
	terminalEmulator := os.Getenv("TERMINAL_EMULATOR")

	supportedTerms := map[string]bool{
		"xterm-kitty": true,
		"foot":        true,
		"foot-extra":  true,
	}

	supportedPrograms := map[string]bool{
		"iTerm.app": true,
		"vscode":    true,
		"WezTerm":   true,
	}

	if supportedTerms[term] {
		return true
	}

	if supportedPrograms[termProgram] {
		return true
	}

	if terminalEmulator == "JetBrains-JediTerm" {
		return true
	}

	if os.Getenv("VTE_VERSION") != "" {
		return true
	}

	if os.Getenv("WT_SESSION") != "" {
		return true
	}

	if strings.Contains(term, "gnome") ||
		strings.Contains(term, "xterm-256color") && termProgram == "gnome-terminal-server" {
		return true
	}

	if termProgram == "Alacritty" || term == "alacritty" {
		return false
	}

	return false
}

// printThrottleDiagnostics shows what the throttling logic would do for debugging
func printThrottleDiagnostics(prItem pr.PullRequest, allActions []actions.Action) {
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
		hasRecent := pr.HasRecentComment(prItem, a.Comment, *throttle)
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

// getLastCommentTime returns when any comment was last posted on the PR
func getLastCommentTime(prItem pr.PullRequest) string {
	if len(prItem.Comments) == 0 {
		return "never"
	}

	// Find the most recent comment (any comment)
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
