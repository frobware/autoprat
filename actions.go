package main

import (
	"fmt"
	"slices"
	"strings"
)

type LabelPredicate int

const (
	PredicateNone LabelPredicate = iota
	PredicateSkipIfLabelExists
	PredicateOnlyIfLabelExists
)

// Action represents a comment command to post on a PR,
// with optional label predicate to control when it applies.
type Action struct {
	Comment   string
	Label     string
	Predicate LabelPredicate
}

// Command returns the gh(1) CLI command string to post this action to
// a PR.
func (a Action) Command(repo string, prNumber int) string {
	escaped := strings.ReplaceAll(a.Comment, `"`, `\"`)
	return fmt.Sprintf(`gh pr comment --repo %s %d --body "%s"`, repo, prNumber, escaped)
}

// FilterActions returns only those actions that should be applied given the PR labels.
func FilterActions(actions []Action, prLabels []string) []Action {
	var filtered []Action

	for _, a := range actions {
		hasLabel := contains(prLabels, a.Label)
		switch a.Predicate {
		case PredicateSkipIfLabelExists:
			if hasLabel {
				continue
			}
		case PredicateOnlyIfLabelExists:
			if !hasLabel {
				continue
			}
		}
		filtered = append(filtered, a)
	}

	return filtered
}

func contains(labels []string, target string) bool {
	return slices.Contains(labels, target)
}
