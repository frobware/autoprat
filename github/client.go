package github

import (
	"slices"
	"strings"
	"time"
)

type Client struct {
	repo string
}

func NewClient(repo string) (*Client, error) {
	return &Client{repo: repo}, nil
}

func (c *Client) List(filter Filter) ([]PullRequest, error) {
	prs, err := fetchPullRequests(c.repo, false)
	if err != nil {
		return nil, err
	}

	filtered := make([]PullRequest, 0, len(prs))

	for _, pr := range prs {
		if matchesFilter(pr, filter) {
			filtered = append(filtered, pr)
		}
	}

	sortPRsDescending(filtered)

	return filtered, nil
}

func matchesFilter(pr PullRequest, filter Filter) bool {
	// Author exact match
	if filter.Author != "" && pr.AuthorLogin != filter.Author {
		return false
	}

	// Author substring match
	if filter.AuthorSubstring != "" && !strings.Contains(pr.AuthorLogin, filter.AuthorSubstring) {
		return false
	}

	// Label filters
	for _, labelFilter := range filter.Labels {
		hasLabel := slices.Contains(pr.Labels, labelFilter.Name)
		if labelFilter.Negate && hasLabel {
			// Should NOT have the label but does.
			return false
		}
		if !labelFilter.Negate && !hasLabel {
			// Should have the label but doesn't.
			return false
		}
	}

	if len(filter.FailingChecks) > 0 && !hasFailingChecks(pr, filter.FailingChecks) {
		return false
	}

	return true
}

func hasFailingCI(pr PullRequest) bool {
	for _, check := range pr.StatusCheckRollup.Contexts.Nodes {
		if check.State == "FAILURE" || check.Conclusion == "FAILURE" {
			return true
		}
	}
	return false
}

func hasFailingChecks(pr PullRequest, checkNames []string) bool {
	for _, targetCheck := range checkNames {
		for _, check := range pr.StatusCheckRollup.Contexts.Nodes {
			checkName := check.Name
			if checkName == "" {
				checkName = check.Context
			}

			// Exact match only for safety.
			if checkName == targetCheck {
				if check.State == "FAILURE" || check.Conclusion == "FAILURE" {
					return true
				}
			}
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
