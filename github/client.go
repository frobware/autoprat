package github

import (
	"fmt"
	"slices"
	"strings"
	"time"

	"github.com/frobware/autoprat/github/search"
)

type Client struct {
	repo string
}

func NewClient(repo string) (*Client, error) {
	return &Client{repo: repo}, nil
}

func (c *Client) Search(query string) ([]PullRequest, error) {
	// Build the complete search query including repository and type
	parts := strings.Split(c.repo, "/")
	if len(parts) != 2 {
		return nil, fmt.Errorf("invalid repository format: %s", c.repo)
	}

	qb := search.NewQueryBuilder().
		Repo(parts[0], parts[1]).
		Type("pr").
		State("open")

	if query != "" {
		qb.AddTerm(query)
	}

	finalQuery := qb.Build()

	prs, err := searchPullRequests(finalQuery)
	if err != nil {
		return nil, err
	}

	sortPRsDescending(prs)
	return prs, nil
}

// hasFailingCI returns true if any CI check is failing.
// Kept for potential future use.
func hasFailingCI(pr PullRequest) bool {
	for _, check := range pr.StatusCheckRollup.Contexts.Nodes {
		if check.State == "FAILURE" || check.Conclusion == "FAILURE" {
			return true
		}
	}
	return false
}

func sortPRsDescending(prs []PullRequest) {
	slices.SortFunc(prs, func(a, b PullRequest) int {
		if b.Number > a.Number {
			return 1
		}
		if b.Number < a.Number {
			return -1
		}
		return 0
	})
}

// HasRecentComment checks if a comment was posted within the throttle period
// using the comments already fetched with the PR data
func HasRecentComment(pr PullRequest, commentText string, throttleWindow time.Duration) bool {
	if throttleWindow <= 0 {
		return false // No throttling means no deduplication.
	}

	cutoff := time.Now().Add(-throttleWindow)

	for _, comment := range pr.Comments {
		if strings.TrimSpace(comment.Body) == strings.TrimSpace(commentText) {
			createdAt, err := time.Parse(time.RFC3339, comment.CreatedAt)
			if err != nil {
				continue // Skip if we can't parse the timestamp.
			}
			if createdAt.After(cutoff) {
				return true // Found recent duplicate.
			}
		}
	}

	return false
}
