package main

import (
	"bytes"
	"io"
	"os"
	"strings"
	"testing"
)

func TestTabularFormatter(t *testing.T) {
	// Capture stdout for testing
	oldStdout := os.Stdout
	r, w, _ := os.Pipe()
	os.Stdout = w

	// Create test data
	pr := PullRequest{
		Number:      123,
		Title:       "Test PR",
		URL:         "https://github.com/test/repo/pull/123",
		AuthorLogin: "testuser",
		AuthorType:  "User",
		Labels:      []string{"approved", "lgtm"},
	}

	result := PRResult{
		RepositoryPRs: []RepositoryPRs{
			{
				Repository: "test/repo",
				PRs:        []PullRequest{pr},
			},
		},
	}

	config := &Config{}

	// Test TabularFormatter
	formatter := &TabularFormatter{}
	err := formatter.Format(result, config)
	if err != nil {
		t.Fatalf("TabularFormatter.Format failed: %v", err)
	}

	// Restore stdout and read captured output
	w.Close()
	os.Stdout = oldStdout

	var buf bytes.Buffer
	io.Copy(&buf, r)
	output := buf.String()

	// Verify output contains expected elements
	if !strings.Contains(output, "REPOSITORY") {
		t.Error("Output should contain header")
	}
	if !strings.Contains(output, "test/repo") {
		t.Error("Output should contain repository name")
	}
	if !strings.Contains(output, "Test PR") {
		t.Error("Output should contain PR title")
	}
	if !strings.Contains(output, "testuser") {
		t.Error("Output should contain author")
	}
}

func TestQuietFormatter(t *testing.T) {
	// Capture stdout for testing
	oldStdout := os.Stdout
	r, w, _ := os.Pipe()
	os.Stdout = w

	// Create test data
	pr1 := PullRequest{Number: 123}
	pr2 := PullRequest{Number: 456}

	result := PRResult{
		RepositoryPRs: []RepositoryPRs{
			{
				Repository: "test/repo",
				PRs:        []PullRequest{pr1, pr2},
			},
		},
	}

	config := &Config{}

	// Test QuietFormatter
	formatter := &QuietFormatter{}
	err := formatter.Format(result, config)
	if err != nil {
		t.Fatalf("QuietFormatter.Format failed: %v", err)
	}

	// Restore stdout and read captured output
	w.Close()
	os.Stdout = oldStdout

	var buf bytes.Buffer
	io.Copy(&buf, r)
	output := buf.String()

	// Verify output contains only PR numbers
	lines := strings.Split(strings.TrimSpace(output), "\n")
	if len(lines) != 2 {
		t.Errorf("Expected 2 lines, got %d", len(lines))
	}
	if lines[0] != "123" {
		t.Errorf("Expected first line to be '123', got '%s'", lines[0])
	}
	if lines[1] != "456" {
		t.Errorf("Expected second line to be '456', got '%s'", lines[1])
	}
}

func TestCommandFormatter(t *testing.T) {
	// Capture stdout for testing
	oldStdout := os.Stdout
	r, w, _ := os.Pipe()
	os.Stdout = w

	// Create test data
	result := CommandResult{
		Commands: []string{
			"gh pr review --approve 123",
			"gh pr review --comment '/lgtm' 456",
		},
	}

	config := &Config{}

	// Test CommandFormatter
	formatter := &CommandFormatter{}
	err := formatter.Format(result, config)
	if err != nil {
		t.Fatalf("CommandFormatter.Format failed: %v", err)
	}

	// Restore stdout and read captured output
	w.Close()
	os.Stdout = oldStdout

	var buf bytes.Buffer
	io.Copy(&buf, r)
	output := buf.String()

	// Verify output contains commands
	if !strings.Contains(output, "gh pr review --approve 123") {
		t.Error("Output should contain first command")
	}
	if !strings.Contains(output, "gh pr review --comment '/lgtm' 456") {
		t.Error("Output should contain second command")
	}
}

func TestFormatResult(t *testing.T) {
	tests := []struct {
		name           string
		result         Result
		config         *Config
		expectError    bool
		expectedFormat string
	}{
		{
			name: "CommandResult",
			result: CommandResult{
				Commands: []string{"test command"},
			},
			config:         &Config{},
			expectError:    false,
			expectedFormat: "command",
		},
		{
			name: "PRResult - Tabular",
			result: PRResult{
				RepositoryPRs: []RepositoryPRs{},
			},
			config:         &Config{},
			expectError:    false,
			expectedFormat: "tabular",
		},
		{
			name: "PRResult - Quiet",
			result: PRResult{
				RepositoryPRs: []RepositoryPRs{},
			},
			config:         &Config{Quiet: true},
			expectError:    false,
			expectedFormat: "quiet",
		},
		{
			name: "PRResult - Verbose",
			result: PRResult{
				RepositoryPRs: []RepositoryPRs{},
			},
			config:         &Config{Detailed: true},
			expectError:    false,
			expectedFormat: "verbose",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Capture stdout to avoid polluting test output
			oldStdout := os.Stdout
			r, w, _ := os.Pipe()
			os.Stdout = w

			err := FormatResult(tt.result, tt.config)

			// Restore stdout
			w.Close()
			os.Stdout = oldStdout
			io.Copy(io.Discard, r) // Discard captured output

			if tt.expectError && err == nil {
				t.Error("Expected error but got none")
			}
			if !tt.expectError && err != nil {
				t.Errorf("Unexpected error: %v", err)
			}
		})
	}
}
