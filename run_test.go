package main_test

import (
	"context"
	"fmt"
	"strings"
	"testing"

	. "github.com/frobware/autoprat"
)

// mockGitHubClient implements GitHubClient for testing
type mockGitHubClient struct {
	searchFunc func(ctx context.Context, query string) ([]PullRequest, error)
}

func (m *mockGitHubClient) Search(ctx context.Context, query string) ([]PullRequest, error) {
	if m.searchFunc != nil {
		return m.searchFunc(ctx, query)
	}
	return []PullRequest{}, nil
}

// mockClientFactory creates mock clients for testing
func mockClientFactory(client GitHubClient) func(repo string) (GitHubClient, error) {
	return func(repo string) (GitHubClient, error) {
		return client, nil
	}
}

func TestRun_SearchQuery(t *testing.T) {
	ctx := context.Background()

	// Setup mock data
	mockPR := PullRequest{
		Number:      123,
		Title:       "Test PR",
		URL:         "https://github.com/owner/repo/pull/123",
		AuthorLogin: "testuser",
	}

	mockClient := &mockGitHubClient{
		searchFunc: func(ctx context.Context, query string) ([]PullRequest, error) {
			// The searchQuery gets passed directly to the client
			return []PullRequest{mockPR}, nil
		},
	}

	config := &Config{
		Repositories: []string{"owner/repo"},
		SearchQuery:  "is:pr is:open",
		Actions:      []Action{},
	}

	result, err := Run(ctx, config, mockClientFactory(mockClient))
	if err != nil {
		t.Fatalf("Run() returned error: %v", err)
	}

	// Should return PRResult for search queries
	prResult, ok := result.(PRResult)
	if !ok {
		t.Fatalf("Expected PRResult, got %T", result)
	}

	if len(prResult.RepositoryPRs) != 1 {
		t.Fatalf("Expected 1 repository, got %d", len(prResult.RepositoryPRs))
	}

	if prResult.RepositoryPRs[0].Repository != "owner/repo" {
		t.Errorf("Expected repository 'owner/repo', got %s", prResult.RepositoryPRs[0].Repository)
	}

	if len(prResult.RepositoryPRs[0].PRs) != 1 {
		t.Fatalf("Expected 1 PR, got %d", len(prResult.RepositoryPRs[0].PRs))
	}

	if prResult.RepositoryPRs[0].PRs[0].Number != 123 {
		t.Errorf("Expected PR number 123, got %d", prResult.RepositoryPRs[0].PRs[0].Number)
	}
}

func TestRun_WithActions(t *testing.T) {
	ctx := context.Background()

	mockPR := PullRequest{
		Number:      456,
		Title:       "Test PR with Actions",
		URL:         "https://github.com/owner/repo/pull/456",
		AuthorLogin: "testuser",
	}

	mockClient := &mockGitHubClient{
		searchFunc: func(ctx context.Context, query string) ([]PullRequest, error) {
			return []PullRequest{mockPR}, nil
		},
	}

	config := &Config{
		Repositories: []string{"owner/repo"},
		SearchQuery:  "is:pr is:open",
		Actions: []Action{
			{Comment: "/approve", Predicate: PredicateNone},
		},
	}

	result, err := Run(ctx, config, mockClientFactory(mockClient))
	if err != nil {
		t.Fatalf("Run() returned error: %v", err)
	}

	// Should return CommandResult when actions are specified
	cmdResult, ok := result.(CommandResult)
	if !ok {
		t.Fatalf("Expected CommandResult, got %T", result)
	}

	if len(cmdResult.Commands) == 0 {
		t.Fatal("Expected commands to be generated, got none")
	}

	// Commands should contain the PR URL and action
	expected := `gh pr comment --repo owner/repo 456 --body "/approve"`
	found := false
	for _, cmd := range cmdResult.Commands {
		if cmd == expected {
			found = true
			break
		}
	}
	if !found {
		t.Errorf("Expected command %q not found in: %v", expected, cmdResult.Commands)
	}
}

func TestRun_MultipleRepositories(t *testing.T) {
	ctx := context.Background()

	mockClient := &mockGitHubClient{
		searchFunc: func(ctx context.Context, query string) ([]PullRequest, error) {
			// Return different PRs based on which client is created
			// In real usage, the query would be built per repo
			return []PullRequest{{Number: 1, Title: "PR1"}}, nil
		},
	}

	config := &Config{
		Repositories: []string{"owner/repo1", "owner/repo2"},
		SearchQuery:  "is:pr is:open",
		Actions:      []Action{},
	}

	result, err := Run(ctx, config, mockClientFactory(mockClient))
	if err != nil {
		t.Fatalf("Run() returned error: %v", err)
	}

	prResult, ok := result.(PRResult)
	if !ok {
		t.Fatalf("Expected PRResult, got %T", result)
	}

	if len(prResult.RepositoryPRs) != 2 {
		t.Fatalf("Expected 2 repositories, got %d", len(prResult.RepositoryPRs))
	}

	// Verify both repositories are present
	repos := make(map[string]bool)
	for _, repoPRs := range prResult.RepositoryPRs {
		repos[repoPRs.Repository] = true
	}

	if !repos["owner/repo1"] || !repos["owner/repo2"] {
		t.Error("Expected both repo1 and repo2 to be present")
	}
}

func TestRun_SpecificPRs(t *testing.T) {
	ctx := context.Background()

	mockClient := &mockGitHubClient{
		searchFunc: func(ctx context.Context, query string) ([]PullRequest, error) {
			// Return all PRs, filtering happens in applyPRFiltering
			return []PullRequest{
				{Number: 123, Title: "Specific PR 1"},
				{Number: 456, Title: "Specific PR 2"},
				{Number: 789, Title: "Other PR"},
			}, nil
		},
	}

	config := &Config{
		Repositories: []string{"owner/repo"},
		ParsedPRs: []PullRequestRef{
			{Number: 123, Repo: ""},
			{Number: 456, Repo: ""},
		},
		Actions: []Action{},
	}

	result, err := Run(ctx, config, mockClientFactory(mockClient))
	if err != nil {
		t.Fatalf("Run() returned error: %v", err)
	}

	prResult, ok := result.(PRResult)
	if !ok {
		t.Fatalf("Expected PRResult, got %T", result)
	}

	if len(prResult.RepositoryPRs) != 1 {
		t.Fatalf("Expected 1 repository, got %d", len(prResult.RepositoryPRs))
	}

	if len(prResult.RepositoryPRs[0].PRs) != 2 {
		t.Fatalf("Expected 2 PRs, got %d", len(prResult.RepositoryPRs[0].PRs))
	}
}

func TestRun_ClientFactoryError(t *testing.T) {
	ctx := context.Background()

	errorClientFactory := func(repo string) (GitHubClient, error) {
		return nil, fmt.Errorf("mock client factory error")
	}

	config := &Config{
		Repositories: []string{"owner/repo"},
		SearchQuery:  "is:pr is:open",
		Actions:      []Action{},
	}

	_, err := Run(ctx, config, errorClientFactory)
	if err == nil {
		t.Fatal("Expected error from client factory, got nil")
	}

	if !strings.Contains(err.Error(), "failed to fetch PRs") {
		t.Errorf("Expected 'failed to fetch PRs' in error, got: %v", err)
	}
}

func TestRun_EmptyRepositories(t *testing.T) {
	ctx := context.Background()

	mockClient := &mockGitHubClient{}

	config := &Config{
		Repositories: []string{},
		SearchQuery:  "is:pr is:open",
		Actions:      []Action{},
	}

	result, err := Run(ctx, config, mockClientFactory(mockClient))
	if err != nil {
		t.Fatalf("Run() returned error: %v", err)
	}

	prResult, ok := result.(PRResult)
	if !ok {
		t.Fatalf("Expected PRResult, got %T", result)
	}

	if len(prResult.RepositoryPRs) != 0 {
		t.Fatalf("Expected 0 repositories, got %d", len(prResult.RepositoryPRs))
	}
}
