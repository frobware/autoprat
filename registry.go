package main

import (
	"embed"
	"fmt"
	"os"
	"path/filepath"
	"slices"
	"strings"

	"gopkg.in/yaml.v3"
)

//go:embed embedded/*.yaml
var embeddedActions embed.FS

// ActionDefinition represents an action definition loaded from YAML.
type ActionDefinition struct {
	Name        string `yaml:"name"`
	Flag        string `yaml:"flag"`
	Description string `yaml:"description"`
	Comment     string `yaml:"comment"`
	Label       string `yaml:"label"`
	Predicate   string `yaml:"predicate"`
	Source      string `yaml:"-"` // "embedded" or "user", not serialized.
}

// ToAction converts an ActionDefinition to the runtime Action type.
func (ad ActionDefinition) ToAction() Action {
	var predicate LabelPredicate
	switch ad.Predicate {
	case "skip_if_label_exists":
		predicate = PredicateSkipIfLabelExists
	case "only_if_label_exists":
		predicate = PredicateOnlyIfLabelExists
	default:
		predicate = PredicateNone
	}

	return Action{
		Comment:   ad.Comment,
		Label:     ad.Label,
		Predicate: predicate,
	}
}

// ActionLoadMode controls what action sources to load.
type ActionLoadMode int

const (
	ActionLoadNothing  ActionLoadMode = 0 // Load nothing
	ActionLoadEmbedded ActionLoadMode = 1 // Load embedded actions only
	ActionLoadUser     ActionLoadMode = 2 // Load user actions only
	ActionLoadAll      ActionLoadMode = 3 // Load embedded + user actions
)

// Registry holds all available actions loaded from embedded and user
// sources.
type Registry struct {
	actions map[string]ActionDefinition
}

// NewRegistry creates a new action registry and loads all available
// actions.
func NewRegistry() (*Registry, error) {
	return NewRegistryWithMode(ActionLoadAll)
}

// NewRegistryWithMode creates a new action registry with specified load mode.
func NewRegistryWithMode(mode ActionLoadMode) (*Registry, error) {
	r := &Registry{
		actions: make(map[string]ActionDefinition),
	}

	// Load embedded actions if requested.
	if mode&ActionLoadEmbedded != 0 {
		if err := r.loadEmbeddedActions(); err != nil {
			return nil, fmt.Errorf("failed to load embedded actions: %w", err)
		}
	}

	// Load user actions if requested.
	if mode&ActionLoadUser != 0 {
		if err := r.loadUserActions(); err != nil {
			// User actions are optional, so we only warn on errors.
			fmt.Fprintf(os.Stderr, "Warning: failed to load user actions: %v\n", err)
		}
	}

	return r, nil
}

// loadEmbeddedActions loads actions from the embedded filesystem.
func (r *Registry) loadEmbeddedActions() error {
	entries, err := embeddedActions.ReadDir("embedded")
	if err != nil {
		return fmt.Errorf("failed to read embedded actions directory: %w", err)
	}

	for _, entry := range entries {
		if !strings.HasSuffix(entry.Name(), ".yaml") {
			continue
		}

		content, err := embeddedActions.ReadFile("embedded/" + entry.Name())
		if err != nil {
			return fmt.Errorf("failed to read embedded action file %s: %w", entry.Name(), err)
		}

		var action ActionDefinition
		if err := yaml.Unmarshal(content, &action); err != nil {
			return fmt.Errorf("failed to parse embedded action file %s: %w", entry.Name(), err)
		}

		if err := r.validateAction(action); err != nil {
			return fmt.Errorf("invalid embedded action %s: %w", entry.Name(), err)
		}

		action.Source = "embedded"
		r.actions[action.Flag] = action
	}

	return nil
}

// loadUserActions loads actions from the user's config directory.
func (r *Registry) loadUserActions() error {
	homeDir, err := os.UserHomeDir()
	if err != nil {
		return fmt.Errorf("failed to get user home directory: %w", err)
	}

	actionsDir := filepath.Join(homeDir, ".config", "autoprat", "actions")

	// Check if directory exists.
	if _, err := os.Stat(actionsDir); os.IsNotExist(err) {
		// Directory doesn't exist, which is fine.
		return nil
	}

	entries, err := os.ReadDir(actionsDir)
	if err != nil {
		return fmt.Errorf("failed to read user actions directory: %w", err)
	}

	for _, entry := range entries {
		if !strings.HasSuffix(entry.Name(), ".yaml") {
			continue
		}

		content, err := os.ReadFile(filepath.Join(actionsDir, entry.Name()))
		if err != nil {
			return fmt.Errorf("failed to read user action file %s: %w", entry.Name(), err)
		}

		var action ActionDefinition
		if err := yaml.Unmarshal(content, &action); err != nil {
			return fmt.Errorf("failed to parse user action file %s: %w", entry.Name(), err)
		}

		if err := r.validateAction(action); err != nil {
			return fmt.Errorf("invalid user action %s: %w", entry.Name(), err)
		}

		action.Source = "user"
		// User actions can override embedded ones.
		r.actions[action.Flag] = action
	}

	return nil
}

// validateAction ensures an action definition is valid.
func (r *Registry) validateAction(action ActionDefinition) error {
	if action.Name == "" {
		return fmt.Errorf("action name is required")
	}

	if action.Flag == "" {
		return fmt.Errorf("action flag is required")
	}

	if action.Description == "" {
		return fmt.Errorf("action description is required")
	}

	if action.Comment == "" {
		return fmt.Errorf("action comment is required")
	}

	// Validate predicate if specified.
	if action.Predicate != "" {
		validPredicates := []string{"skip_if_label_exists", "only_if_label_exists"}
		valid := slices.Contains(validPredicates, action.Predicate)
		if !valid {
			return fmt.Errorf("invalid predicate %q, must be one of: %s", action.Predicate, strings.Join(validPredicates, ", "))
		}

		// If predicate is specified, label is required.
		if action.Label == "" {
			return fmt.Errorf("label is required when predicate is specified")
		}
	}

	return nil
}

// GetAction returns the action definition for the given flag name.
func (r *Registry) GetAction(flag string) (ActionDefinition, bool) {
	action, exists := r.actions[flag]
	return action, exists
}

// GetAllActions returns all loaded action definitions.
func (r *Registry) GetAllActions() map[string]ActionDefinition {
	return r.actions
}

// GetFlags returns all available flag names in sorted order.
func (r *Registry) GetFlags() []string {
	var flags []string

	for flag := range r.actions {
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

// GetFlagsBySource returns flag names for actions from a specific
// source, sorted.
func (r *Registry) GetFlagsBySource(source string) []string {
	var flags []string
	for flag, action := range r.actions {
		if action.Source == source {
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
