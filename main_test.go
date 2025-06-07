package main

import (
	"testing"

	"github.com/frobware/autoprat/github"
)

func TestParsePullRequestRef(t *testing.T) {
	tests := []struct {
		name     string
		arg      string
		expected PullRequestRef
		wantErr  bool
	}{
		{
			name: "numeric PR number",
			arg:  "123",
			expected: PullRequestRef{
				Number: 123,
				Repo:   "",
			},
			wantErr: false,
		},
		{
			name: "GitHub PR URL",
			arg:  "https://github.com/owner/repo/pull/456",
			expected: PullRequestRef{
				Number: 456,
				Repo:   "owner/repo",
			},
			wantErr: false,
		},
		{
			name: "GitHub PR URL with trailing slash",
			arg:  "https://github.com/owner/repo/pull/789/",
			expected: PullRequestRef{
				Number: 789,
				Repo:   "owner/repo",
			},
			wantErr: false,
		},
		{
			name: "GitHub PR URL with complex repo name",
			arg:  "https://github.com/org-name/repo.name/pull/101",
			expected: PullRequestRef{
				Number: 101,
				Repo:   "org-name/repo.name",
			},
			wantErr: false,
		},
		{
			name:    "invalid numeric string",
			arg:     "abc",
			wantErr: true,
		},
		{
			name:    "invalid URL",
			arg:     "not-a-url",
			wantErr: true,
		},
		{
			name:    "invalid GitHub URL - wrong path",
			arg:     "https://github.com/owner/repo/issues/123",
			wantErr: true,
		},
		{
			name:    "invalid GitHub URL - missing PR number",
			arg:     "https://github.com/owner/repo/pull/",
			wantErr: true,
		},
		{
			name:    "invalid GitHub URL - non-numeric PR",
			arg:     "https://github.com/owner/repo/pull/abc",
			wantErr: true,
		},
		{
			name: "GitLab URL with correct path structure",
			arg:  "https://gitlab.com/owner/repo/pull/123",
			expected: PullRequestRef{
				Number: 123,
				Repo:   "owner/repo",
			},
			wantErr: false,
		},
		{
			name: "zero PR number",
			arg:  "0",
			expected: PullRequestRef{
				Number: 0,
				Repo:   "",
			},
			wantErr: false,
		},
		{
			name: "negative PR number",
			arg:  "-123",
			expected: PullRequestRef{
				Number: -123,
				Repo:   "",
			},
			wantErr: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result, err := parsePRArgument(tt.arg)

			if tt.wantErr {
				if err == nil {
					t.Errorf("parsePRArgument(%q) expected error, got nil", tt.arg)
				}
				return
			}

			if err != nil {
				t.Errorf("parsePRArgument(%q) unexpected error: %v", tt.arg, err)
				return
			}

			if result.Number != tt.expected.Number {
				t.Errorf("parsePRArgument(%q).Number = %d, want %d", tt.arg, result.Number, tt.expected.Number)
			}

			if result.Repo != tt.expected.Repo {
				t.Errorf("parsePRArgument(%q).Repo = %q, want %q", tt.arg, result.Repo, tt.expected.Repo)
			}
		})
	}
}

func TestGetBuildInfo(t *testing.T) {
	version, buildTime, goVer := getBuildInfo()

	// These should never be empty
	if version == "" {
		t.Error("getBuildInfo() version should not be empty")
	}

	if buildTime == "" {
		t.Error("getBuildInfo() buildTime should not be empty")
	}

	if goVer == "" {
		t.Error("getBuildInfo() goVer should not be empty")
	}

	// Go version should start with "go"
	if len(goVer) < 2 || goVer[:2] != "go" {
		t.Errorf("getBuildInfo() goVer = %q, should start with 'go'", goVer)
	}
}

func TestRepositoryPRs(t *testing.T) {
	repo := RepositoryPRs{
		Repository: "owner/repo",
		PRs:        []github.PullRequest{},
	}

	if repo.Repository != "owner/repo" {
		t.Errorf("RepositoryPRs.Repository = %q, want %q", repo.Repository, "owner/repo")
	}

	if len(repo.PRs) != 0 {
		t.Errorf("RepositoryPRs.PRs length = %d, want 0", len(repo.PRs))
	}
}

func TestConfig(t *testing.T) {
	config := Config{
		Repositories: []string{"repo1", "repo2"},
		ParsedPRs:    []PullRequestRef{{Number: 123, Repo: "test/repo"}},
		SearchQuery:  "test query",
	}

	if len(config.Repositories) != 2 {
		t.Errorf("Config.Repositories length = %d, want 2", len(config.Repositories))
	}

	if len(config.ParsedPRs) != 1 {
		t.Errorf("Config.ParsedPRs length = %d, want 1", len(config.ParsedPRs))
	}

	if config.SearchQuery != "test query" {
		t.Errorf("Config.SearchQuery = %q, want %q", config.SearchQuery, "test query")
	}
}

func TestContains(t *testing.T) {
	slice := []string{"apple", "banana", "cherry"}

	tests := []struct {
		name     string
		item     string
		expected bool
	}{
		{
			name:     "item exists",
			item:     "banana",
			expected: true,
		},
		{
			name:     "item does not exist",
			item:     "orange",
			expected: false,
		},
		{
			name:     "empty item",
			item:     "",
			expected: false,
		},
		{
			name:     "case sensitive",
			item:     "Banana",
			expected: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := contains(slice, tt.item)
			if result != tt.expected {
				t.Errorf("contains(%v, %q) = %t, want %t", slice, tt.item, result, tt.expected)
			}
		})
	}
}

func TestYesNo(t *testing.T) {
	tests := []struct {
		name     string
		input    bool
		expected string
	}{
		{
			name:     "true returns Yes",
			input:    true,
			expected: "Yes",
		},
		{
			name:     "false returns No",
			input:    false,
			expected: "No",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := yesNo(tt.input)
			if result != tt.expected {
				t.Errorf("yesNo(%t) = %q, want %q", tt.input, result, tt.expected)
			}
		})
	}
}

func TestCIStatus(t *testing.T) {
	tests := []struct {
		name     string
		checks   []github.StatusCheck
		expected string
	}{
		{
			name: "failing check present",
			checks: []github.StatusCheck{
				{State: "SUCCESS"},
				{State: "FAILURE"},
				{State: "PENDING"},
			},
			expected: "Failing",
		},
		{
			name: "failing conclusion present",
			checks: []github.StatusCheck{
				{Conclusion: "SUCCESS"},
				{Conclusion: "FAILURE"},
			},
			expected: "Failing",
		},
		{
			name: "pending check present (no failures)",
			checks: []github.StatusCheck{
				{State: "SUCCESS"},
				{State: "PENDING"},
			},
			expected: "Pending",
		},
		{
			name: "pending conclusion present (no failures)",
			checks: []github.StatusCheck{
				{Conclusion: "SUCCESS"},
				{Conclusion: "PENDING"},
			},
			expected: "Pending",
		},
		{
			name: "all passing",
			checks: []github.StatusCheck{
				{State: "SUCCESS"},
				{Conclusion: "SUCCESS"},
			},
			expected: "Passing",
		},
		{
			name:     "no checks",
			checks:   []github.StatusCheck{},
			expected: "Passing",
		},
		{
			name: "empty state uses conclusion",
			checks: []github.StatusCheck{
				{State: "", Conclusion: "FAILURE"},
			},
			expected: "Failing",
		},
		{
			name: "mixed state and conclusion",
			checks: []github.StatusCheck{
				{State: "SUCCESS", Conclusion: ""},
				{State: "", Conclusion: "PENDING"},
			},
			expected: "Pending",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Create a PullRequest with the checks to test the method
			pr := github.PullRequest{
				StatusCheckRollup: github.StatusCheckRollup{
					Contexts: struct {
						Nodes []github.StatusCheck `json:"nodes"`
					}{Nodes: tt.checks},
				},
			}
			result := pr.CIStatus()
			if result != tt.expected {
				t.Errorf("CIStatus() = %q, want %q", result, tt.expected)
			}
		})
	}
}

func TestLastCommentTime(t *testing.T) {
	tests := []struct {
		name     string
		pr       github.PullRequest
		expected string
	}{
		{
			name: "no comments",
			pr: github.PullRequest{
				Comments: []github.Comment{},
			},
			expected: "never",
		},
		{
			name: "single comment",
			pr: github.PullRequest{
				Comments: []github.Comment{
					{
						CreatedAt: "2023-01-01T12:00:00Z",
					},
				},
			},
			// This will be a duration - we'll just check it's not "never"
			expected: "",
		},
		{
			name: "multiple comments",
			pr: github.PullRequest{
				Comments: []github.Comment{
					{
						CreatedAt: "2023-01-01T12:00:00Z",
					},
					{
						CreatedAt: "2023-01-02T12:00:00Z",
					},
				},
			},
			expected: "",
		},
		{
			name: "invalid timestamp",
			pr: github.PullRequest{
				Comments: []github.Comment{
					{
						CreatedAt: "invalid-timestamp",
					},
				},
			},
			expected: "never",
		},
		{
			name: "mixed valid and invalid timestamps",
			pr: github.PullRequest{
				Comments: []github.Comment{
					{
						CreatedAt: "invalid-timestamp",
					},
					{
						CreatedAt: "2023-01-01T12:00:00Z",
					},
				},
			},
			expected: "",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := tt.pr.LastCommentTime()

			if tt.expected == "never" {
				if result != "never" {
					t.Errorf("LastCommentTime() = %q, want %q", result, "never")
				}
			} else if tt.expected == "" {
				// For valid timestamps, we just check it's not "never"
				if result == "never" {
					t.Errorf("LastCommentTime() = 'never', expected a duration")
				}
			} else {
				if result != tt.expected {
					t.Errorf("LastCommentTime() = %q, want %q", result, tt.expected)
				}
			}
		})
	}
}
