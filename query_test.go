package main

import (
	"testing"
)

func TestQueryBuilder(t *testing.T) {
	tests := []struct {
		name     string
		build    func() *QueryBuilder
		expected string
	}{
		{
			name: "basic repo and type",
			build: func() *QueryBuilder {
				return NewQueryBuilder().
					Repo("owner", "repo").
					Type("pr")
			},
			expected: "repo:owner/repo type:pr",
		},
		{
			name: "full basic query",
			build: func() *QueryBuilder {
				return NewQueryBuilder().
					Repo("owner", "repo").
					Type("pr").
					State("open")
			},
			expected: "repo:owner/repo type:pr state:open",
		},
		{
			name: "author filter",
			build: func() *QueryBuilder {
				return NewQueryBuilder().
					Repo("owner", "repo").
					Type("pr").
					Author("dependabot")
			},
			expected: "repo:owner/repo type:pr author:dependabot",
		},
		{
			name: "single label",
			build: func() *QueryBuilder {
				return NewQueryBuilder().
					Repo("owner", "repo").
					Type("pr").
					Label("bug")
			},
			expected: "repo:owner/repo type:pr label:bug",
		},
		{
			name: "label negation",
			build: func() *QueryBuilder {
				return NewQueryBuilder().
					Repo("owner", "repo").
					Type("pr").
					NoLabel("hold")
			},
			expected: "repo:owner/repo type:pr -label:hold",
		},
		{
			name: "multiple labels",
			build: func() *QueryBuilder {
				return NewQueryBuilder().
					Repo("owner", "repo").
					Type("pr").
					Label("bug").
					Label("priority/high").
					NoLabel("hold")
			},
			expected: "repo:owner/repo type:pr label:bug label:priority/high -label:hold",
		},
		{
			name: "review filters",
			build: func() *QueryBuilder {
				return NewQueryBuilder().
					Repo("owner", "repo").
					Type("pr").
					ReviewRequired()
			},
			expected: "repo:owner/repo type:pr review:required",
		},
		{
			name: "review approved",
			build: func() *QueryBuilder {
				return NewQueryBuilder().
					Repo("owner", "repo").
					Type("pr").
					ReviewApproved()
			},
			expected: "repo:owner/repo type:pr review:approved",
		},
		{
			name: "status filter",
			build: func() *QueryBuilder {
				return NewQueryBuilder().
					Repo("owner", "repo").
					Type("pr").
					Status("failure")
			},
			expected: "repo:owner/repo type:pr status:failure",
		},
		{
			name: "draft filter - is draft",
			build: func() *QueryBuilder {
				return NewQueryBuilder().
					Repo("owner", "repo").
					Type("pr").
					Draft(true)
			},
			expected: "repo:owner/repo type:pr is:draft",
		},
		{
			name: "draft filter - not draft",
			build: func() *QueryBuilder {
				return NewQueryBuilder().
					Repo("owner", "repo").
					Type("pr").
					Draft(false)
			},
			expected: "repo:owner/repo type:pr -is:draft",
		},
		{
			name: "sort filter",
			build: func() *QueryBuilder {
				return NewQueryBuilder().
					Repo("owner", "repo").
					Type("pr").
					Sort("updated")
			},
			expected: "repo:owner/repo type:pr sort:updated",
		},
		{
			name: "complex query with all features",
			build: func() *QueryBuilder {
				return NewQueryBuilder().
					Repo("owner", "repo").
					Type("pr").
					State("open").
					Author("dependabot").
					Label("dependencies").
					NoLabel("hold").
					Status("failure").
					Draft(false).
					ReviewRequired()
			},
			expected: "repo:owner/repo type:pr state:open author:dependabot label:dependencies -label:hold status:failure -is:draft review:required",
		},
		{
			name: "raw term addition",
			build: func() *QueryBuilder {
				return NewQueryBuilder().
					Repo("owner", "repo").
					Type("pr").
					AddTerm("custom:term")
			},
			expected: "repo:owner/repo type:pr custom:term",
		},
		{
			name: "empty query builder",
			build: func() *QueryBuilder {
				return NewQueryBuilder()
			},
			expected: "",
		},
		{
			name: "only raw terms",
			build: func() *QueryBuilder {
				return NewQueryBuilder().
					AddTerm("term1").
					AddTerm("term2")
			},
			expected: "term1 term2",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			qb := tt.build()
			result := qb.Build()
			if result != tt.expected {
				t.Errorf("QueryBuilder.Build() = %q, want %q", result, tt.expected)
			}
		})
	}
}

func TestParseQuery(t *testing.T) {
	tests := []struct {
		name     string
		query    string
		expected string
	}{
		{
			name:     "empty query",
			query:    "",
			expected: "",
		},
		{
			name:     "simple query",
			query:    "test query",
			expected: "test query",
		},
		{
			name:     "complex query",
			query:    "author:user label:bug -label:hold",
			expected: "author:user label:bug -label:hold",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			qb := ParseQuery(tt.query)
			result := qb.Build()
			if result != tt.expected {
				t.Errorf("ParseQuery(%q).Build() = %q, want %q", tt.query, result, tt.expected)
			}
		})
	}
}

func TestQueryBuilderChaining(t *testing.T) {
	// Test that all methods return the QueryBuilder for chaining
	qb := NewQueryBuilder()

	// All these should be chainable
	result := qb.
		Repo("owner", "repo").
		Type("pr").
		State("open").
		Author("user").
		Label("bug").
		NoLabel("hold").
		ReviewRequired().
		ReviewApproved().
		Status("success").
		Draft(true).
		Sort("updated").
		AddTerm("custom")

	if result != qb {
		t.Error("QueryBuilder methods should return the same instance for chaining")
	}
}

func TestQueryBuilderEdgeCases(t *testing.T) {
	tests := []struct {
		name     string
		build    func() *QueryBuilder
		expected string
	}{
		{
			name: "empty strings",
			build: func() *QueryBuilder {
				return NewQueryBuilder().
					Repo("", "").
					Author("").
					Label("").
					AddTerm("")
			},
			expected: "repo:/ author: label: ",
		},
		{
			name: "special characters in repo",
			build: func() *QueryBuilder {
				return NewQueryBuilder().
					Repo("owner-name", "repo.name")
			},
			expected: "repo:owner-name/repo.name",
		},
		{
			name: "special characters in author",
			build: func() *QueryBuilder {
				return NewQueryBuilder().
					Author("user-name")
			},
			expected: "author:user-name",
		},
		{
			name: "special characters in labels",
			build: func() *QueryBuilder {
				return NewQueryBuilder().
					Label("priority/high").
					NoLabel("do-not-merge/hold")
			},
			expected: "label:priority/high -label:do-not-merge/hold",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			qb := tt.build()
			result := qb.Build()
			if result != tt.expected {
				t.Errorf("QueryBuilder.Build() = %q, want %q", result, tt.expected)
			}
		})
	}
}
