package github

import (
	"strings"

	_ "embed"

	"github.com/cli/go-gh"
)

//go:embed queries/search-prs.graphql
var searchPRQuery string

// graphQLPullRequest matches the GraphQL query response structure.
type graphQLPullRequest struct {
	Number            int               `json:"number"`
	Title             string            `json:"title"`
	HeadRefName       string            `json:"headRefName"`
	CreatedAt         string            `json:"createdAt"`
	State             string            `json:"state"`
	Author            author            `json:"author"`
	Labels            labels            `json:"labels"`
	URL               string            `json:"url"`
	StatusCheckRollup StatusCheckRollup `json:"statusCheckRollup"`
	Comments          comments          `json:"comments"`
}

type comments struct {
	Nodes []Comment `json:"nodes"`
}

type author struct {
	Login string `json:"login"`
}

type labels struct {
	Nodes []struct {
		Name string `json:"name"`
	} `json:"nodes"`
}

func searchPullRequests(query string) ([]PullRequest, error) {
	client, err := gh.GQLClient(nil)
	if err != nil {
		return nil, err
	}

	vars := map[string]any{
		"query": query,
	}

	var resp struct {
		Search struct {
			Nodes []graphQLPullRequest `json:"nodes"`
		} `json:"search"`
	}

	if err := client.Do(searchPRQuery, vars, &resp); err != nil {
		return nil, err
	}

	prs := make([]PullRequest, 0, len(resp.Search.Nodes))
	for _, gqlPR := range resp.Search.Nodes {
		labelNames := make([]string, 0, len(gqlPR.Labels.Nodes))
		for _, label := range gqlPR.Labels.Nodes {
			labelNames = append(labelNames, label.Name)
		}

		// Extract repo from URL since search doesn't include repo context
		repo := extractRepoFromURL(gqlPR.URL)

		pr := PullRequest{
			Number:            gqlPR.Number,
			Title:             gqlPR.Title,
			HeadRefName:       gqlPR.HeadRefName,
			CreatedAt:         gqlPR.CreatedAt,
			State:             gqlPR.State,
			Labels:            labelNames,
			AuthorLogin:       gqlPR.Author.Login,
			URL:               gqlPR.URL,
			StatusCheckRollup: gqlPR.StatusCheckRollup,
			Comments:          gqlPR.Comments.Nodes,
			repo:              repo,
		}
		prs = append(prs, pr)
	}

	return prs, nil
}

func extractRepoFromURL(url string) string {
	// Extract owner/repo from https://github.com/owner/repo/pull/123
	parts := strings.Split(url, "/")
	if len(parts) >= 5 && parts[2] == "github.com" {
		return parts[3] + "/" + parts[4]
	}
	return ""
}
