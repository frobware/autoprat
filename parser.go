package main

import (
	"fmt"
	"net/url"
	"regexp"
	"strconv"
)

// pullRequestRefRegex matches GitHub PR URLs like "/owner/repo/pull/123" or "/owner/repo/pull/123/".
var pullRequestRefRegex = regexp.MustCompile(`^/([^/]+)/([^/]+)/pull/(\d+)/?$`)

// ParsePRArgument extracts a PR number and repository from either a numeric string or GitHub URL.
func ParsePRArgument(arg string) (PullRequestRef, error) {
	if num, err := strconv.Atoi(arg); err == nil {
		return PullRequestRef{Number: num}, nil
	}

	parsedURL, err := url.Parse(arg)
	if err != nil {
		return PullRequestRef{}, fmt.Errorf("invalid PR number or URL %q", arg)
	}

	matches := pullRequestRefRegex.FindStringSubmatch(parsedURL.Path)
	if len(matches) != 4 {
		return PullRequestRef{}, fmt.Errorf("invalid GitHub PR URL %q", arg)
	}

	urlRepo := matches[1] + "/" + matches[2]
	prNumber, err := strconv.Atoi(matches[3])
	if err != nil {
		return PullRequestRef{}, fmt.Errorf("invalid PR number in URL %q", arg)
	}

	return PullRequestRef{Number: prNumber, Repo: urlRepo}, nil
}
