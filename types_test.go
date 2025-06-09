package main_test

import (
	"testing"

	. "github.com/frobware/autoprat"
)

func TestPullRequest_CIStatus(t *testing.T) {
	tests := []struct {
		name     string
		checks   []StatusCheck
		expected string
	}{
		{
			name: "failing check present",
			checks: []StatusCheck{
				{State: "SUCCESS"},
				{State: "FAILURE"},
				{State: "PENDING"},
			},
			expected: "Failing",
		},
		{
			name: "failing conclusion present",
			checks: []StatusCheck{
				{Conclusion: "SUCCESS"},
				{Conclusion: "FAILURE"},
			},
			expected: "Failing",
		},
		{
			name: "pending check present (no failures)",
			checks: []StatusCheck{
				{State: "SUCCESS"},
				{State: "PENDING"},
			},
			expected: "Pending",
		},
		{
			name: "pending conclusion present (no failures)",
			checks: []StatusCheck{
				{Conclusion: "SUCCESS"},
				{Conclusion: "PENDING"},
			},
			expected: "Pending",
		},
		{
			name: "all passing",
			checks: []StatusCheck{
				{State: "SUCCESS"},
				{Conclusion: "SUCCESS"},
			},
			expected: "Passing",
		},
		{
			name:     "no checks",
			checks:   []StatusCheck{},
			expected: "Passing",
		},
		{
			name: "empty state uses conclusion",
			checks: []StatusCheck{
				{State: "", Conclusion: "FAILURE"},
			},
			expected: "Failing",
		},
		{
			name: "mixed state and conclusion",
			checks: []StatusCheck{
				{State: "SUCCESS", Conclusion: ""},
				{State: "", Conclusion: "PENDING"},
			},
			expected: "Pending",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			pr := PullRequest{
				StatusCheckRollup: StatusCheckRollup{
					Contexts: struct {
						Nodes []StatusCheck `json:"nodes"`
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

func TestPullRequest_LastCommentTime(t *testing.T) {
	tests := []struct {
		name     string
		pr       PullRequest
		expected string
	}{
		{
			name: "no comments",
			pr: PullRequest{
				Comments: []Comment{},
			},
			expected: "never",
		},
		{
			name: "single comment",
			pr: PullRequest{
				Comments: []Comment{
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
			pr: PullRequest{
				Comments: []Comment{
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
			pr: PullRequest{
				Comments: []Comment{
					{
						CreatedAt: "invalid-timestamp",
					},
				},
			},
			expected: "never",
		},
		{
			name: "mixed valid and invalid timestamps",
			pr: PullRequest{
				Comments: []Comment{
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
