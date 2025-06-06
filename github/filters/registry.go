package filters

import (
	"embed"
	"fmt"
	"os"
	"path/filepath"
	"slices"
	"strings"

	"github.com/frobware/autoprat/github"
	"gopkg.in/yaml.v3"
)

//go:embed embedded/*.yaml
var embeddedFilters embed.FS

// FilterDefinition represents a filter definition loaded from YAML.
type FilterDefinition struct {
	Name        string `yaml:"name"`
	Flag        string `yaml:"flag"`
	FlagShort   string `yaml:"flag_short,omitempty"`
	Description string `yaml:"description"`
	FilterType  string `yaml:"filter_type"`
	Label       string `yaml:"label,omitempty"`
	Source      string `yaml:"-"` // "embedded" or "user", not serialized.
}

// FilterType represents the type of filter logic to apply.
type FilterType int

const (
	FilterTypeLabelAbsence FilterType = iota
	FilterTypeLabelPresence
	FilterTypeFailingCI
)

// ToFilterType converts a string filter type to the enum value.
func ToFilterType(s string) FilterType {
	switch s {
	case "label_absence":
		return FilterTypeLabelAbsence
	case "label_presence":
		return FilterTypeLabelPresence
	case "failing_ci":
		return FilterTypeFailingCI
	default:
		return FilterTypeLabelAbsence
	}
}

// Apply applies this filter to a slice of PRs and returns the filtered results.
func (fd FilterDefinition) Apply(prs []github.PullRequest) []github.PullRequest {
	switch ToFilterType(fd.FilterType) {
	case FilterTypeLabelAbsence:
		return filterByLabelAbsence(prs, fd.Label)
	case FilterTypeLabelPresence:
		return filterByLabelPresence(prs, fd.Label)
	case FilterTypeFailingCI:
		return filterByFailingCI(prs)
	default:
		return prs
	}
}

// filterByLabelAbsence returns PRs that don't have the specified label.
func filterByLabelAbsence(prs []github.PullRequest, label string) []github.PullRequest {
	filtered := prs[:0]
	for _, pr := range prs {
		if !slices.Contains(pr.Labels, label) {
			filtered = append(filtered, pr)
		}
	}
	return filtered
}

// filterByLabelPresence returns PRs that have the specified label.
func filterByLabelPresence(prs []github.PullRequest, label string) []github.PullRequest {
	filtered := prs[:0]
	for _, pr := range prs {
		if slices.Contains(pr.Labels, label) {
			filtered = append(filtered, pr)
		}
	}
	return filtered
}

// filterByFailingCI returns PRs that have failing CI checks.
func filterByFailingCI(prs []github.PullRequest) []github.PullRequest {
	filtered := prs[:0]
	for _, pr := range prs {
		for _, check := range pr.StatusCheckRollup.Contexts.Nodes {
			status := check.State
			if status == "" {
				status = check.Conclusion
			}
			if status == "FAILURE" {
				filtered = append(filtered, pr)
				break
			}
		}
	}
	return filtered
}

// Registry holds all available filters loaded from embedded and user sources.
type Registry struct {
	filters map[string]FilterDefinition
}

// NewRegistry creates a new filter registry and loads all available filters.
func NewRegistry() (*Registry, error) {
	r := &Registry{
		filters: make(map[string]FilterDefinition),
	}

	if err := r.loadEmbeddedFilters(); err != nil {
		return nil, fmt.Errorf("failed to load embedded filters: %w", err)
	}

	if err := r.loadUserFilters(); err != nil {
		fmt.Fprintf(os.Stderr, "Warning: failed to load user filters: %v\n", err)
	}

	return r, nil
}

// loadEmbeddedFilters loads filters from the embedded filesystem.
func (r *Registry) loadEmbeddedFilters() error {
	entries, err := embeddedFilters.ReadDir("embedded")
	if err != nil {
		return fmt.Errorf("failed to read embedded filters directory: %w", err)
	}

	for _, entry := range entries {
		if !strings.HasSuffix(entry.Name(), ".yaml") {
			continue
		}

		content, err := embeddedFilters.ReadFile("embedded/" + entry.Name())
		if err != nil {
			return fmt.Errorf("failed to read embedded filter file %s: %w", entry.Name(), err)
		}

		var filter FilterDefinition
		if err := yaml.Unmarshal(content, &filter); err != nil {
			return fmt.Errorf("failed to parse embedded filter file %s: %w", entry.Name(), err)
		}

		if err := r.validateFilter(filter); err != nil {
			return fmt.Errorf("invalid embedded filter %s: %w", entry.Name(), err)
		}

		filter.Source = "embedded"
		r.filters[filter.Flag] = filter
	}

	return nil
}

// loadUserFilters loads filters from the user's config directory.
func (r *Registry) loadUserFilters() error {
	homeDir, err := os.UserHomeDir()
	if err != nil {
		return fmt.Errorf("failed to get user home directory: %w", err)
	}

	filtersDir := filepath.Join(homeDir, ".config", "autoprat", "filters")

	if _, err := os.Stat(filtersDir); os.IsNotExist(err) {
		return nil
	}

	entries, err := os.ReadDir(filtersDir)
	if err != nil {
		return fmt.Errorf("failed to read user filters directory: %w", err)
	}

	for _, entry := range entries {
		if !strings.HasSuffix(entry.Name(), ".yaml") {
			continue
		}

		content, err := os.ReadFile(filepath.Join(filtersDir, entry.Name()))
		if err != nil {
			return fmt.Errorf("failed to read user filter file %s: %w", entry.Name(), err)
		}

		var filter FilterDefinition
		if err := yaml.Unmarshal(content, &filter); err != nil {
			return fmt.Errorf("failed to parse user filter file %s: %w", entry.Name(), err)
		}

		if err := r.validateFilter(filter); err != nil {
			return fmt.Errorf("invalid user filter %s: %w", entry.Name(), err)
		}

		filter.Source = "user"
		r.filters[filter.Flag] = filter
	}

	return nil
}

// validateFilter ensures a filter definition is valid.
func (r *Registry) validateFilter(filter FilterDefinition) error {
	if filter.Name == "" {
		return fmt.Errorf("filter name is required")
	}
	if filter.Flag == "" {
		return fmt.Errorf("filter flag is required")
	}
	if filter.Description == "" {
		return fmt.Errorf("filter description is required")
	}
	if filter.FilterType == "" {
		return fmt.Errorf("filter type is required")
	}

	validTypes := []string{"label_absence", "label_presence", "failing_ci"}
	if !slices.Contains(validTypes, filter.FilterType) {
		return fmt.Errorf("invalid filter type %q, must be one of: %s", filter.FilterType, strings.Join(validTypes, ", "))
	}

	if (filter.FilterType == "label_absence" || filter.FilterType == "label_presence") && filter.Label == "" {
		return fmt.Errorf("label is required for filter type %q", filter.FilterType)
	}

	return nil
}

// GetFilter returns the filter definition for the given flag name.
func (r *Registry) GetFilter(flag string) (FilterDefinition, bool) {
	filter, exists := r.filters[flag]
	return filter, exists
}

// GetAllFilters returns all loaded filter definitions.
func (r *Registry) GetAllFilters() map[string]FilterDefinition {
	return r.filters
}

// GetFlags returns all available filter flag names in sorted order.
func (r *Registry) GetFlags() []string {
	var flags []string
	for flag := range r.filters {
		flags = append(flags, flag)
	}

	for i := range len(flags) - 1 {
		for j := i + 1; j < len(flags); j++ {
			if flags[i] > flags[j] {
				flags[i], flags[j] = flags[j], flags[i]
			}
		}
	}
	return flags
}

// GetFlagsBySource returns filter flag names for a specific source, sorted.
func (r *Registry) GetFlagsBySource(source string) []string {
	var flags []string
	for flag, filter := range r.filters {
		if filter.Source == source {
			flags = append(flags, flag)
		}
	}

	for i := range len(flags) - 1 {
		for j := i + 1; j < len(flags); j++ {
			if flags[i] > flags[j] {
				flags[i], flags[j] = flags[j], flags[i]
			}
		}
	}
	return flags
}
