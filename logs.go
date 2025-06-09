package main

import (
	"fmt"
	"io"
	"net/http"
	"regexp"
	"strings"

	"github.com/cli/go-gh"
)

// FetchCheckLogs retrieves and filters error logs from a failing check
func (pr *PullRequest) FetchCheckLogs(check StatusCheck) (string, error) {
	url := check.DetailsUrl
	if url == "" {
		url = check.TargetUrl
	}
	if url == "" {
		return "", fmt.Errorf("no URL available for check logs")
	}

	if strings.Contains(url, "prow.ci.openshift.org/view/gs/") {
		url = strings.Replace(url, "prow.ci.openshift.org/view/gs/", "storage.googleapis.com/", 1)
		if !strings.HasSuffix(url, "/build-log.txt") {
			url = url + "/build-log.txt"
		}
	} else if strings.Contains(url, "github.com") && strings.Contains(url, "#issuecomment") {
		return "", fmt.Errorf("GitHub comment URL does not contain raw logs")
	} else if !strings.Contains(url, "storage.googleapis.com") && !strings.Contains(url, "raw") {
		return "", fmt.Errorf("URL does not appear to contain raw logs: %s", url)
	}

	client, err := gh.HTTPClient(nil)
	if err != nil {
		return "", fmt.Errorf("failed to create HTTP client: %w", err)
	}

	resp, err := client.Get(url)
	if err != nil {
		return "", fmt.Errorf("failed to fetch check logs from %s: %w", url, err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return "", fmt.Errorf("received status %d from %s", resp.StatusCode, url)
	}

	body, err := io.ReadAll(resp.Body)
	if err != nil {
		return "", fmt.Errorf("failed to read response body: %w", err)
	}

	return filterErrorLogs(string(body)), nil
}

// filterErrorLogs extracts lines that look like errors from log content
func filterErrorLogs(content string) string {
	lines := strings.Split(content, "\n")
	var errorLines []string

	errorPatterns := []*regexp.Regexp{
		regexp.MustCompile(`(?i)(error|failed|failure|fatal|panic):`),
		regexp.MustCompile(`(?i)\b(error|fail|exception)\b`),
		regexp.MustCompile(`^\s*\+\s*.*error`),
		regexp.MustCompile(`^\s*E\s+`),
		regexp.MustCompile(`^\s*FAIL\s+`),
		regexp.MustCompile(`exit\s+code\s+[1-9]`),
	}

	for _, line := range lines {
		line = strings.TrimSpace(line)
		if line == "" {
			continue
		}

		if len(line) > 500 {
			continue
		}

		for _, pattern := range errorPatterns {
			if pattern.MatchString(line) {
				errorLines = append(errorLines, "    "+line)
				break
			}
		}
	}

	if len(errorLines) > 20 {
		errorLines = errorLines[:20]
		errorLines = append(errorLines, "    ... (truncated)")
	}

	if len(errorLines) == 0 {
		return ""
	}

	return strings.Join(errorLines, "\n")
}
