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
	debug           = pflag.Bool("debug", false, "Enable debug logging")
	failingCI       = pflag.BoolP("failing-ci", "f", false, "Only show PRs with failing CI")
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
	showVersion     = pflag.Bool("version", false, "Show version information")
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
		for _, pr := range prs {
			toPost := actions.FilterActions(allActions, pr.Labels)
			for _, a := range toPost {
				fmt.Println(a.Command(*repo, pr.Number))
			}
		}
		return
	}

	if *verbose || *verboseVerbose {
		for _, pr := range prs {
			printVerbosePR(pr, *verboseVerbose, *noHyperlinks)
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
	fmt.Fprintln(tw, "PR URL\tCI\tAPPROVED\tLGTM\tOK2TEST\tHOLD\tAUTHOR\tTITLE")
	for _, pr := range prs {
		approved := contains(pr.Labels, "approved")
		lgtm := contains(pr.Labels, "lgtm")
		okToTest := contains(pr.Labels, "needs-ok-to-test")
		hold := contains(pr.Labels, "do-not-merge/hold")

		ciStatus := summarizeCIStatus(pr.StatusCheckRollup.Contexts.Nodes)

		fmt.Fprintf(tw, "%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n",
			pr.URL,
			ciStatus,
			yesNo(approved),
			yesNo(lgtm),
			yesNo(!okToTest), // ok-to-test = Y if no 'needs-ok-to-test' label
			yesNo(hold),
			pr.AuthorLogin,
			pr.Title,
		)
	}
	tw.Flush()
}

func printVerbosePR(pr pr.PullRequest, showLogs bool, noHyperlinks bool) {
	fmt.Printf("● %s\n", pr.URL)
	fmt.Printf("├─Title: %s (%s)\n", pr.Title, pr.AuthorLogin)
	fmt.Printf("├─PR #%d\n", pr.Number)
	fmt.Printf("├─State: %s\n", pr.State)
	fmt.Printf("├─Created: %s\n", pr.CreatedAt)
	fmt.Printf("├─Status\n")

	approved := contains(pr.Labels, "approved")
	lgtm := contains(pr.Labels, "lgtm")
	okToTest := contains(pr.Labels, "needs-ok-to-test")

	fmt.Printf("│ ├─Approved: %s\n", yesNo(approved))
	fmt.Printf("│ ├─CI: %s\n", summarizeCIStatus(pr.StatusCheckRollup.Contexts.Nodes))
	fmt.Printf("│ ├─LGTM: %s\n", yesNo(lgtm))
	fmt.Printf("│ └─OK-to-test: %s\n", yesNo(!okToTest))

	fmt.Printf("├─Labels\n")
	if len(pr.Labels) == 0 {
		fmt.Printf("│ └─None\n")
	} else {
		for i, label := range pr.Labels {
			prefix := "│ ├─"
			if i == len(pr.Labels)-1 {
				prefix = "│ └─"
			}
			fmt.Printf("%s%s\n", prefix, label)
		}
	}

	fmt.Printf("└─Checks\n")
	checks := pr.StatusCheckRollup.Contexts.Nodes
	if len(checks) == 0 {
		fmt.Println("  └─None")
	} else {
		for i, check := range checks {
			prefix := "├─"
			if i == len(checks)-1 {
				prefix = "└─"
			}
			name := check.Name
			if name == "" {
				name = check.Context
			}
			conclusion := check.Conclusion
			if conclusion == "" {
				conclusion = check.State
			}
			statusText := conclusion
			url := check.DetailsUrl
			if url == "" {
				url = check.TargetUrl
			}

			supportsHyperlinks := !noHyperlinks && term.IsTerminal(int(os.Stdout.Fd())) && terminalSupportsHyperlinks()

			if url != "" && supportsHyperlinks {
				statusText = fmt.Sprintf("\033]8;;%s\033\\%s\033]8;;\033\\", url, conclusion)
			}

			fmt.Printf("  %s%s: %s\n", prefix, name, statusText)

			if url != "" && !supportsHyperlinks {
				fmt.Printf("    URL: %s\n", url)
			}

			if showLogs && (conclusion == "FAILURE" || check.State == "FAILURE") {
				if logs, err := pr.FetchCheckLogs(check); err == nil && logs != "" {
					fmt.Printf("    Error logs:\n%s\n", logs)
				} else if err != nil {
					fmt.Printf("    (Could not fetch logs: %v)\n", err)
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
