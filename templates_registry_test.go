package main

import (
	"testing"
)

func TestNewTemplateRegistryWithMode(t *testing.T) {
	tests := []struct {
		name           string
		mode           TemplateLoadMode
		expectEmpty    bool
		expectEmbedded bool
	}{
		{
			name:        "LoadNothing",
			mode:        TemplateLoadNothing,
			expectEmpty: true,
		},
		{
			name:           "LoadEmbedded",
			mode:           TemplateLoadEmbedded,
			expectEmbedded: true,
		},
		{
			name:           "LoadAll",
			mode:           TemplateLoadAll,
			expectEmbedded: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			registry, err := NewTemplateRegistryWithMode(tt.mode)
			if err != nil {
				t.Fatalf("NewTemplateRegistryWithMode(%v) failed: %v", tt.mode, err)
			}

			templates := registry.GetAllTemplates()

			if tt.expectEmpty && len(templates) != 0 {
				t.Errorf("Expected empty registry, got %d templates", len(templates))
			}

			if tt.expectEmbedded {
				// Should have at least some embedded templates
				if len(templates) == 0 {
					t.Errorf("Expected embedded templates, got none")
				}

				// Check that embedded templates have correct source
				foundEmbedded := false
				for _, template := range templates {
					if template.Source == "embedded" {
						foundEmbedded = true
						break
					}
				}
				if !foundEmbedded {
					t.Errorf("Expected to find embedded templates with Source='embedded'")
				}
			}
		})
	}
}
