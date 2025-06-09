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
	"context"
	"log"
)

func main() {
	// Create registries and extract plain data
	actionRegistry, err := NewRegistry()
	if err != nil {
		log.Fatalf("Failed to load action registry: %v", err)
	}

	templateRegistry, err := NewTemplateRegistry()
	if err != nil {
		log.Fatalf("Failed to load template registry: %v", err)
	}

	// Extract plain data structures for dependency injection
	availableActions := actionRegistry.GetAllActions()
	availableTemplates := templateRegistry.GetAllTemplates()

	// Parse command-line arguments with injected data
	config, err := Parse(availableActions, availableTemplates)
	if err != nil {
		log.Fatalf("CLI parsing failed: %v", err)
	}

	// Create context
	ctx := context.Background()

	// Create client factory
	clientFactory := func(repo string) (GitHubClient, error) {
		return NewClient(repo)
	}

	// Run the core application logic
	result, err := Run(ctx, config, clientFactory)
	if err != nil {
		log.Fatalf("%v", err)
	}

	// Handle output using the output package
	if err := FormatResult(result, config); err != nil {
		log.Fatalf("Failed to format output: %v", err)
	}
}
