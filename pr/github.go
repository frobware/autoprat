package pr

import (
	"encoding/json"
	"fmt"
	"strings"

	_ "embed"

	"github.com/cli/go-gh"
)

//go:embed queries/list-prs.graphql
var listPRQuery string

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

func fetchPullRequests(repo string, debug bool) ([]PullRequest, error) {
	parts := strings.SplitN(repo, "/", 2)
	if len(parts) != 2 {
		return nil, fmt.Errorf("invalid repo: %q", repo)
	}
	owner, name := parts[0], parts[1]

	client, err := gh.GQLClient(nil)
	if err != nil {
		return nil, err
	}

	vars := map[string]any{
		"owner": owner,
		"repo":  name,
	}

	var resp struct {
		Repository struct {
			PullRequests struct {
				Nodes []graphQLPullRequest
			}
		}
	}

	if err := client.Do(listPRQuery, vars, &resp); err != nil {
		return nil, err
	}

	if debug {
		for i, node := range resp.Repository.PullRequests.Nodes {
			data, _ := json.MarshalIndent(node, "", "  ")
			fmt.Printf("DEBUG: PR #%d:\n%s\n\n", i+1, string(data))
		}
	}

	prs := make([]PullRequest, 0, len(resp.Repository.PullRequests.Nodes))
	for _, gqlPR := range resp.Repository.PullRequests.Nodes {
		labelNames := make([]string, 0, len(gqlPR.Labels.Nodes))
		for _, label := range gqlPR.Labels.Nodes {
			labelNames = append(labelNames, label.Name)
		}

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
