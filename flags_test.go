package main

import (
	"testing"
)

func TestBuildQuery(t *testing.T) {
	tests := []struct {
		name     string
		template QueryTemplate
		value    string
		values   []string
		expected string
		wantErr  bool
	}{
		{
			name: "non-parameterized template",
			template: QueryTemplate{
				Query:         "is:pr is:open",
				Parameterized: false,
			},
			expected: "is:pr is:open",
			wantErr:  false,
		},
		{
			name: "parameterized template with value",
			template: QueryTemplate{
				QueryTemplate: "author:{value}",
				Parameterized: true,
			},
			value:    "testuser",
			expected: "author:testuser",
			wantErr:  false,
		},
		{
			name: "parameterized template missing query_template",
			template: QueryTemplate{
				Parameterized: true,
			},
			wantErr: true,
		},
		{
			name: "label template with single value",
			template: QueryTemplate{
				QueryTemplate: "{labels}",
				Parameterized: true,
			},
			values:   []string{"bug"},
			expected: "label:bug",
			wantErr:  false,
		},
		{
			name: "label template with multiple values",
			template: QueryTemplate{
				QueryTemplate: "{labels}",
				Parameterized: true,
			},
			values:   []string{"bug", "enhancement"},
			expected: "label:bug label:enhancement",
			wantErr:  false,
		},
		{
			name: "label template with negation",
			template: QueryTemplate{
				QueryTemplate: "{labels}",
				Parameterized: true,
			},
			values:   []string{"-bug", "enhancement"},
			expected: "-label:bug label:enhancement",
			wantErr:  false,
		},
		{
			name: "combined value and labels",
			template: QueryTemplate{
				QueryTemplate: "author:{value} {labels}",
				Parameterized: true,
			},
			value:    "testuser",
			values:   []string{"bug", "-wip"},
			expected: "author:testuser label:bug -label:wip",
			wantErr:  false,
		},
		{
			name: "empty labels array",
			template: QueryTemplate{
				QueryTemplate: "{labels}",
				Parameterized: true,
			},
			values:   []string{},
			expected: "",
			wantErr:  false,
		},
		{
			name: "only negated labels",
			template: QueryTemplate{
				QueryTemplate: "{labels}",
				Parameterized: true,
			},
			values:   []string{"-bug", "-wip"},
			expected: "-label:bug -label:wip",
			wantErr:  false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result, err := BuildQuery(tt.template, tt.value, tt.values)
			if (err != nil) != tt.wantErr {
				t.Errorf("buildQuery() error = %v, wantErr %v", err, tt.wantErr)
				return
			}
			if result != tt.expected {
				t.Errorf("buildQuery() = %q, want %q", result, tt.expected)
			}
		})
	}
}

func TestBuildQuery_FlagsEdgeCases(t *testing.T) {
	tests := []struct {
		name     string
		template QueryTemplate
		value    string
		values   []string
		expected string
	}{
		{
			name: "value with spaces",
			template: QueryTemplate{
				QueryTemplate: "author:{value}",
				Parameterized: true,
			},
			value:    "test user",
			expected: "author:test user",
		},
		{
			name: "label with special characters",
			template: QueryTemplate{
				QueryTemplate: "{labels}",
				Parameterized: true,
			},
			values:   []string{"needs-review", "high-priority"},
			expected: "label:needs-review label:high-priority",
		},
		{
			name: "empty value substitution",
			template: QueryTemplate{
				QueryTemplate: "author:{value}",
				Parameterized: true,
			},
			value:    "",
			expected: "author:",
		},
		{
			name: "template with no substitutions",
			template: QueryTemplate{
				QueryTemplate: "static query",
				Parameterized: true,
			},
			expected: "static query",
		},
		{
			name: "multiple occurrences of {value}",
			template: QueryTemplate{
				QueryTemplate: "{value} OR assignee:{value}",
				Parameterized: true,
			},
			value:    "testuser",
			expected: "testuser OR assignee:testuser",
		},
		{
			name: "labels with leading/trailing whitespace",
			template: QueryTemplate{
				QueryTemplate: "{labels}",
				Parameterized: true,
			},
			values:   []string{" bug ", "  enhancement  ", "-wip"},
			expected: "label: bug  label:  enhancement   -label:wip",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result, err := BuildQuery(tt.template, tt.value, tt.values)
			if err != nil {
				t.Errorf("buildQuery() unexpected error: %v", err)
				return
			}
			if result != tt.expected {
				t.Errorf("buildQuery() = %q, want %q", result, tt.expected)
			}
		})
	}
}
