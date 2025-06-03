package pr

// PullRequest represents a minimal view of a GitHub PR for filtering,
// listing, and acting on.
type PullRequest struct {
	Number            int
	Title             string
	HeadRefName       string
	CreatedAt         string
	Labels            []string
	AuthorLogin       string
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

type LabelFilter struct {
	Name   string
	Negate bool
}

// Filter expresses optional match criteria for PR selection.
type Filter struct {
	Author          string
	AuthorSubstring string
	Labels          []LabelFilter
	OnlyFailingCI   bool
	FailingChecks   []string
}
