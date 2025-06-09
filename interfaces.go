package main

import (
	"context"
)

// GitHubClient defines the interface for GitHub API operations.
type GitHubClient interface {
	Search(ctx context.Context, query string) ([]PullRequest, error)
}
