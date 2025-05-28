package pr

import (
	"slices"
	"strings"
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

	if filter.OnlyFailingCI && !hasFailingCI(pr) {
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
