package main

import (
	"fmt"
	"strings"
)

// QueryBuilder constructs GitHub search syntax for pull requests.
type QueryBuilder struct {
	terms []string
}

// NewQueryBuilder creates a new query builder.
func NewQueryBuilder() *QueryBuilder {
	return &QueryBuilder{}
}

// Repo adds a repository filter.
func (qb *QueryBuilder) Repo(owner, name string) *QueryBuilder {
	qb.terms = append(qb.terms, fmt.Sprintf("repo:%s/%s", owner, name))
	return qb
}

// Type adds the type filter for pull requests.
func (qb *QueryBuilder) Type(t string) *QueryBuilder {
	qb.terms = append(qb.terms, fmt.Sprintf("type:%s", t))
	return qb
}

// State adds a state filter.
func (qb *QueryBuilder) State(state string) *QueryBuilder {
	qb.terms = append(qb.terms, fmt.Sprintf("state:%s", state))
	return qb
}

// Author adds an author filter.
func (qb *QueryBuilder) Author(author string) *QueryBuilder {
	qb.terms = append(qb.terms, fmt.Sprintf("author:%s", author))
	return qb
}

// Label adds a label filter.
func (qb *QueryBuilder) Label(label string) *QueryBuilder {
	qb.terms = append(qb.terms, fmt.Sprintf("label:%s", label))
	return qb
}

// NoLabel adds a negative label filter.
func (qb *QueryBuilder) NoLabel(label string) *QueryBuilder {
	qb.terms = append(qb.terms, fmt.Sprintf("-label:%s", label))
	return qb
}

// ReviewRequired adds a filter for PRs that need review.
func (qb *QueryBuilder) ReviewRequired() *QueryBuilder {
	qb.terms = append(qb.terms, "review:required")
	return qb
}

// ReviewApproved adds a filter for PRs that are approved.
func (qb *QueryBuilder) ReviewApproved() *QueryBuilder {
	qb.terms = append(qb.terms, "review:approved")
	return qb
}

// Status adds a status filter for CI checks.
func (qb *QueryBuilder) Status(status string) *QueryBuilder {
	qb.terms = append(qb.terms, fmt.Sprintf("status:%s", status))
	return qb
}

// Draft adds a filter for draft PRs.
func (qb *QueryBuilder) Draft(isDraft bool) *QueryBuilder {
	if isDraft {
		qb.terms = append(qb.terms, "is:draft")
	} else {
		qb.terms = append(qb.terms, "-is:draft")
	}
	return qb
}

// Sort adds sorting criteria.
func (qb *QueryBuilder) Sort(field string) *QueryBuilder {
	qb.terms = append(qb.terms, fmt.Sprintf("sort:%s", field))
	return qb
}

// AddTerm adds a raw search term.
func (qb *QueryBuilder) AddTerm(term string) *QueryBuilder {
	qb.terms = append(qb.terms, term)
	return qb
}

// Build constructs the final search query string.
func (qb *QueryBuilder) Build() string {
	return strings.Join(qb.terms, " ")
}

// ParseQuery parses a query string and creates a QueryBuilder.
// This handles both simple queries and complex expressions.
func ParseQuery(query string) *QueryBuilder {
	qb := NewQueryBuilder()

	// For now, just add the raw query
	// TODO: Implement proper parsing for complex expressions
	if query != "" {
		qb.AddTerm(query)
	}

	return qb
}
