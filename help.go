package main

import (
	"fmt"
	"strings"
)

// PrintHelpFromFlags prints help text using flag definitions
func PrintHelpFromFlags(programName string, categories []FlagCategory) {
	helpText, err := RenderHelpFromFlags(programName, categories)
	if err != nil {
		fmt.Printf("Error rendering help: %v\n", err)
		return
	}
	fmt.Print(helpText)
}

// RenderHelpFromFlags renders help text from flag categories
func RenderHelpFromFlags(programName string, categories []FlagCategory) (string, error) {
	var result strings.Builder

	// Write the header
	result.WriteString(fmt.Sprintf(`Usage: %s [flags] [PR-NUMBER|PR-URL ...]

List and filter open GitHub pull requests.

Filter PRs and generate gh(1) commands to apply /lgtm, /approve,
/ok-to-test, and custom comments.

By default, lists all open PRs when --repo is specified alone.
Optionally focus on specific PRs by providing:
  - PR numbers (e.g. "123", requires --repo)
  - GitHub URLs (e.g. "https://github.com/owner/repo/pull/123")

When using GitHub URLs, the repository is extracted from the URL
automatically and --repo is not required.

`, programName))

	// Find the maximum flag display width for consistent alignment
	maxWidth := 0
	for _, category := range categories {
		for _, flag := range category.Flags {
			flagDisplay := flag.Display()
			if len(flagDisplay) > maxWidth {
				maxWidth = len(flagDisplay)
			}
		}
	}

	// Ensure minimum width for readability
	if maxWidth < 20 {
		maxWidth = 20
	}

	// Write each category
	for _, category := range categories {
		result.WriteString(fmt.Sprintf("%s\n", category.Name))

		for _, flag := range category.Flags {
			flagDisplay := flag.Display()
			padding := strings.Repeat(" ", maxWidth-len(flagDisplay)+2)
			result.WriteString(fmt.Sprintf("  %s%s%s\n", flagDisplay, padding, flag.Description))
		}
		result.WriteString("\n")
	}

	// Write examples section
	result.WriteString(fmt.Sprintf(`
Examples:
  # List all open PRs in a repository.
  %s -r owner/repo

  # Filter PRs that need approval.
  %s -r owner/repo --needs-approve

  # Generate approval commands for Dependabot PRs.
  %s -r owner/repo --author dependabot --approve

  # Execute the generated commands.
  %s -r owner/repo --author dependabot --approve | sh

  # Focus on specific PRs.
  %s -r owner/repo --detailed 123 456
  %s --detailed https://github.com/owner/repo/pull/123`,
		programName, programName, programName,
		programName, programName, programName))

	return result.String(), nil
}
