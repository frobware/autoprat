package main

import (
	"context"
	"fmt"
	"sync"
)

// RepositoryPRs holds PRs from a specific repository.
type RepositoryPRs struct {
	Repository string
	PRs        []PullRequest
}

// Result represents the output of running the application.
type Result interface{}

// PRResult contains PR data to be displayed.
type PRResult struct {
	RepositoryPRs []RepositoryPRs
}

// CommandResult contains commands to be executed.
type CommandResult struct {
	Commands []string
}

// Run executes the main application logic and returns structured data.
func Run(ctx context.Context, config *Config, clientFactory func(repo string) (GitHubClient, error)) (Result, error) {
	// Fetch PRs from all repositories
	allRepositoryPRs, err := fetchAllRepositoryPRsWithSearch(ctx, config.Repositories, config.SearchQuery, clientFactory)
	if err != nil {
		return nil, fmt.Errorf("failed to fetch PRs: %w", err)
	}

	// Apply PR-specific filtering
	filteredPRs := applyPRFiltering(allRepositoryPRs, config)

	// Determine result type based on config
	if len(config.Actions) > 0 {
		// Generate commands
		var commands []string
		for _, repoPRs := range filteredPRs {
			for _, prItem := range repoPRs.PRs {
				toPost := FilterActions(config.Actions, prItem.Labels)
				for _, a := range toPost {
					if config.Throttle > 0 && HasRecentComment(prItem, a.Comment, config.Throttle) {
						// Skip throttled comments - could add debug info to result
						continue
					}
					commands = append(commands, a.Command(repoPRs.Repository, prItem.Number))
				}
			}
		}
		return CommandResult{Commands: commands}, nil
	}

	// Return PR data for display
	return PRResult{RepositoryPRs: filteredPRs}, nil
}

// fetchAllRepositoryPRsWithSearch fetches PRs from all repositories using the search API.
func fetchAllRepositoryPRsWithSearch(ctx context.Context, repositories []string, searchQuery string, clientFactory func(repo string) (GitHubClient, error)) ([]RepositoryPRs, error) {
	var allRepositoryPRs []RepositoryPRs
	var wg sync.WaitGroup
	var mu sync.Mutex
	errChan := make(chan error, len(repositories))

	for _, repository := range repositories {
		wg.Add(1)
		go func(repo string) {
			defer wg.Done()

			client, err := clientFactory(repo)
			if err != nil {
				errChan <- fmt.Errorf("failed to create client for %s: %v", repo, err)
				return
			}

			prs, err := client.Search(ctx, searchQuery)
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

	// Sort results for consistent ordering
	for i := 0; i < len(allRepositoryPRs); i++ {
		for j := i + 1; j < len(allRepositoryPRs); j++ {
			if allRepositoryPRs[i].Repository > allRepositoryPRs[j].Repository {
				allRepositoryPRs[i], allRepositoryPRs[j] = allRepositoryPRs[j], allRepositoryPRs[i]
			}
		}
	}

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
				var filtered []PullRequest
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
