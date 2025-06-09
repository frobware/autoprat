package main

import (
	"testing"
)

func TestNewRegistryWithMode(t *testing.T) {
	tests := []struct {
		name           string
		mode           ActionLoadMode
		expectEmpty    bool
		expectEmbedded bool
	}{
		{
			name:        "LoadNothing",
			mode:        ActionLoadNothing,
			expectEmpty: true,
		},
		{
			name:           "LoadEmbedded",
			mode:           ActionLoadEmbedded,
			expectEmbedded: true,
		},
		{
			name:           "LoadAll",
			mode:           ActionLoadAll,
			expectEmbedded: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			registry, err := NewRegistryWithMode(tt.mode)
			if err != nil {
				t.Fatalf("NewRegistryWithMode(%v) failed: %v", tt.mode, err)
			}

			actions := registry.GetAllActions()

			if tt.expectEmpty && len(actions) != 0 {
				t.Errorf("Expected empty registry, got %d actions", len(actions))
			}

			if tt.expectEmbedded {
				// Should have at least some embedded actions
				if len(actions) == 0 {
					t.Errorf("Expected embedded actions, got none")
				}

				// Check that embedded actions have correct source
				foundEmbedded := false
				for _, action := range actions {
					if action.Source == "embedded" {
						foundEmbedded = true
						break
					}
				}
				if !foundEmbedded {
					t.Errorf("Expected to find embedded actions with Source='embedded'")
				}
			}
		})
	}
}
