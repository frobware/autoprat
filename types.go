package main

import (
	"fmt"
	"strings"
	"time"
)

// PullRequestRef represents a reference to a pull request.
type PullRequestRef struct {
	Number int
	Repo   string // Empty for numeric arguments, populated for URLs.
}

// PullRequest represents a minimal view of a GitHub PR for filtering,
// listing, and acting on.
type PullRequest struct {
	Number            int
	Title             string
	HeadRefName       string
	CreatedAt         string
	Labels            []string
	AuthorLogin       string
	AuthorType        string
	URL               string
	State             string
	StatusCheckRollup StatusCheckRollup
	Comments          []Comment
	repo              string
}

type StatusCheckRollup struct {
	Contexts struct {
		Nodes []StatusCheck `json:"nodes"`
	} `json:"contexts"`
}

type StatusCheck struct {
	Context    string `json:"context,omitempty"`
	Name       string `json:"name,omitempty"`
	State      string `json:"state"`
	Conclusion string `json:"conclusion,omitempty"`
	DetailsUrl string `json:"detailsUrl,omitempty"`
	TargetUrl  string `json:"targetUrl,omitempty"`
}

type Comment struct {
	Body      string `json:"body"`
	CreatedAt string `json:"createdAt"`
	Author    struct {
		Login string `json:"login"`
	} `json:"author"`
}

// LastCommentTime returns when any comment was last posted on the PR.
func (pr PullRequest) LastCommentTime() string {
	if len(pr.Comments) == 0 {
		return "never"
	}

	// Find the most recent comment (any comment).
	var mostRecent time.Time
	found := false

	for _, comment := range pr.Comments {
		createdAt, err := time.Parse(time.RFC3339, comment.CreatedAt)
		if err != nil {
			continue
		}
		if !found || createdAt.After(mostRecent) {
			mostRecent = createdAt
			found = true
		}
	}

	if !found {
		return "never"
	}

	timeSince := time.Since(mostRecent)
	if timeSince < time.Minute {
		return fmt.Sprintf("%ds", int(timeSince.Seconds()))
	} else if timeSince < time.Hour {
		return fmt.Sprintf("%dm", int(timeSince.Minutes()))
	} else if timeSince < 24*time.Hour {
		return fmt.Sprintf("%dh%dm", int(timeSince.Hours()), int(timeSince.Minutes())%60)
	} else {
		return fmt.Sprintf("%dd", int(timeSince.Hours()/24))
	}
}

// CIStatus returns a summary of the CI status for the pull request.
func (pr PullRequest) CIStatus() string {
	checks := pr.StatusCheckRollup.Contexts.Nodes
	for _, c := range checks {
		st := c.State
		if st == "" {
			st = c.Conclusion
		}
		if st == "FAILURE" || st == "ACTION_REQUIRED" {
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

// Author returns the author name for display purposes.
// For bots, shows the full "app/botname" format to match search expectations.
func (pr PullRequest) Author() string {
	if pr.AuthorType == "Bot" {
		return "app/" + pr.AuthorLogin
	}
	return pr.AuthorLogin
}

// SearchAuthorName returns the author name in the format expected by GitHub search.
// For bots, GitHub search expects "app/botname" but GraphQL returns just "botname".
func (pr PullRequest) SearchAuthorName() string {
	if pr.AuthorType == "Bot" {
		return "app/" + pr.AuthorLogin
	}
	return pr.AuthorLogin
}

// PrintThrottleDiagnostics shows what the throttling logic would do for debugging.
func (pr PullRequest) PrintThrottleDiagnostics(allActions []Action, throttle time.Duration) {
	toPost := FilterActions(allActions, pr.Labels)
	if len(toPost) == 0 {
		return
	}

	fmt.Printf("├─Throttle Analysis (period: %v)\n", throttle)

	if len(pr.Comments) == 0 {
		fmt.Printf("│ └─No recent comments found\n")
		for _, a := range toPost {
			fmt.Printf("│   └─Would post: %s ✓\n", a.Comment)
		}
		return
	}

	fmt.Printf("│ ├─Recent comments (%d found):\n", len(pr.Comments))

	for i, comment := range pr.Comments {
		createdAt, err := time.Parse(time.RFC3339, comment.CreatedAt)
		age := ""
		if err == nil {
			age = time.Since(createdAt).Round(time.Minute).String()
		}

		prefix := "│ │ ├─"
		if i == len(pr.Comments)-1 {
			prefix = "│ │ └─"
		}

		fmt.Printf("%s%s (%s ago): %q\n", prefix, comment.Author.Login, age,
			strings.ReplaceAll(strings.TrimSpace(comment.Body), "\n", " "))
	}

	fmt.Printf("│ └─Action Analysis:\n")
	for i, a := range toPost {
		hasRecent := HasRecentComment(pr, a.Comment, throttle)
		status := "✓ Would post"
		if hasRecent {
			status = "✗ Throttled (recent duplicate)"
		}

		prefix := "│   ├─"
		if i == len(toPost)-1 {
			prefix = "│   └─"
		}

		fmt.Printf("%s%s: %s\n", prefix, status, a.Comment)
	}
}
