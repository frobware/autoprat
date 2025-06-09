package main

import (
	"testing"
)

func TestParsePRArgument(t *testing.T) {
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
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result, err := ParsePRArgument(tt.arg)

			if tt.wantErr {
				if err == nil {
					t.Errorf("ParsePRArgument(%q) expected error, got nil", tt.arg)
				}
				return
			}

			if err != nil {
				t.Errorf("ParsePRArgument(%q) unexpected error: %v", tt.arg, err)
				return
			}

			if result.Number != tt.expected.Number {
				t.Errorf("ParsePRArgument(%q).Number = %d, want %d", tt.arg, result.Number, tt.expected.Number)
			}

			if result.Repo != tt.expected.Repo {
				t.Errorf("ParsePRArgument(%q).Repo = %q, want %q", tt.arg, result.Repo, tt.expected.Repo)
			}
		})
	}
}
