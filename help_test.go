package main

import (
	"os"
	"testing"

	"github.com/frobware/autoprat/github/actions"
	"github.com/frobware/autoprat/github/search"
)

func TestRenderHelpComplete(t *testing.T) {
	// Create registries and use DefineAllFlags to get real data
	actionRegistry, err := actions.NewRegistry()
	if err != nil {
		t.Fatalf("Failed to create action registry: %v", err)
	}

	templateRegistry, err := search.NewTemplateRegistry()
	if err != nil {
		t.Fatalf("Failed to create template registry: %v", err)
	}

	categories := DefineAllFlags(actionRegistry, templateRegistry)

	// Filter to only embedded actions and templates (ignore user-defined ones)
	var filteredCategories []FlagCategory
	for _, cat := range categories {
		if cat.Name == "Repository:" || cat.Name == "Output:" || cat.Name == "Utility:" {
			filteredCategories = append(filteredCategories, cat)
		} else if cat.Name == "Filters:" {
			// Only include embedded filters
			var embeddedFilters []FlagInfo
			for _, flag := range cat.Flags {
				template, exists := templateRegistry.GetTemplate(flag.Name)
				if exists && template.Source == "embedded" {
					embeddedFilters = append(embeddedFilters, flag)
				}
			}
			if len(embeddedFilters) > 0 {
				filteredCategories = append(filteredCategories, FlagCategory{
					Name:  "Filters:",
					Flags: embeddedFilters,
				})
			}
		} else if cat.Name == "Actions:" {
			// Include embedded actions plus comment/throttle flags
			var embeddedActions []FlagInfo
			for _, flag := range cat.Flags {
				// Include comment and throttle flags (not from registry)
				if flag.Name == "comment" || flag.Name == "throttle" {
					embeddedActions = append(embeddedActions, flag)
					continue
				}
				// Include embedded actions from registry
				action, exists := actionRegistry.GetAction(flag.Name)
				if exists && action.Source == "embedded" {
					embeddedActions = append(embeddedActions, flag)
				}
			}
			if len(embeddedActions) > 0 {
				filteredCategories = append(filteredCategories, FlagCategory{
					Name:  "Actions:",
					Flags: embeddedActions,
				})
			}
		}
	}

	result, err := RenderHelpFromFlags("./autoprat", filteredCategories)
	if err != nil {
		t.Fatalf("RenderHelpFromFlags failed: %v", err)
	}

	// Read expected output from testdata
	expected, err := os.ReadFile("testdata/help_complete.txt")
	if err != nil {
		t.Fatalf("Failed to read testdata: %v", err)
	}

	if result != string(expected) {
		t.Errorf("Help output mismatch.\nExpected:\n%s\nGot:\n%s", string(expected), result)
	}
}

func TestFlagDisplay(t *testing.T) {
	tests := []struct {
		name     string
		flag     FlagInfo
		expected string
	}{
		{
			name: "simple bool flag",
			flag: FlagInfo{
				Name:      "simple",
				ShortName: "",
				Type:      "bool",
			},
			expected: "--simple",
		},
		{
			name: "bool flag with short",
			flag: FlagInfo{
				Name:      "verbose",
				ShortName: "v",
				Type:      "bool",
			},
			expected: "-v, --verbose",
		},
		{
			name: "string flag with short",
			flag: FlagInfo{
				Name:      "author",
				ShortName: "a",
				Type:      "string",
			},
			expected: "-a, --author string",
		},
		{
			name: "stringSlice flag",
			flag: FlagInfo{
				Name:      "label",
				ShortName: "l",
				Type:      "stringSlice",
			},
			expected: "-l, --label strings",
		},
		{
			name: "duration flag",
			flag: FlagInfo{
				Name:      "throttle",
				ShortName: "",
				Type:      "duration",
			},
			expected: "--throttle duration",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := tt.flag.Display()
			if result != tt.expected {
				t.Errorf("Expected %q, got %q", tt.expected, result)
			}
		})
	}
}

func TestDefineAllFlags(t *testing.T) {
	actionRegistry, err := actions.NewRegistry()
	if err != nil {
		t.Fatalf("Failed to create action registry: %v", err)
	}

	templateRegistry, err := search.NewTemplateRegistry()
	if err != nil {
		t.Fatalf("Failed to create template registry: %v", err)
	}

	categories := DefineAllFlags(actionRegistry, templateRegistry)

	// Verify we have the expected sections
	expectedSections := map[string]bool{
		"Repository:": true,
		"Filters:":    true,
		"Actions:":    true,
		"Output:":     true,
		"Utility:":    true,
	}

	foundSections := make(map[string]bool)
	for _, cat := range categories {
		foundSections[cat.Name] = true
	}

	for section := range expectedSections {
		if !foundSections[section] {
			t.Errorf("Expected section %q not found", section)
		}
	}

	// Verify Repository section has repo flag
	for _, cat := range categories {
		if cat.Name == "Repository:" {
			found := false
			for _, flag := range cat.Flags {
				if flag.Name == "repo" && flag.ShortName == "r" && flag.Type == "string" {
					found = true
					break
				}
			}
			if !found {
				t.Error("Repository section should contain repo flag with short name 'r' and type 'string'")
			}
			break
		}
	}
}
