package main

import (
	"os"
	"path/filepath"
	"testing"
)

func TestQueryTemplate(t *testing.T) {
	template := QueryTemplate{
		Name:             "Test Template",
		Flag:             "test-flag",
		FlagShort:        "t",
		Description:      "A test template",
		Query:            "test:query",
		Parameterized:    false,
		SupportsMultiple: false,
		Source:           "embedded",
	}

	if template.Name != "Test Template" {
		t.Errorf("Expected Name to be 'Test Template', got %q", template.Name)
	}
	if template.Flag != "test-flag" {
		t.Errorf("Expected Flag to be 'test-flag', got %q", template.Flag)
	}
	if template.Source != "embedded" {
		t.Errorf("Expected Source to be 'embedded', got %q", template.Source)
	}
}

func TestTemplateRegistry_BuildQuery(t *testing.T) {
	// Create a test registry
	registry := &TemplateRegistry{
		templates: make(map[string]QueryTemplate),
	}

	// Add test templates
	registry.templates["simple"] = QueryTemplate{
		Name:          "Simple",
		Flag:          "simple",
		Query:         "simple:query",
		Parameterized: false,
	}

	registry.templates["author"] = QueryTemplate{
		Name:             "Author",
		Flag:             "author",
		QueryTemplate:    "author:{value}",
		Parameterized:    true,
		SupportsMultiple: false,
	}

	registry.templates["labels"] = QueryTemplate{
		Name:             "Labels",
		Flag:             "labels",
		QueryTemplate:    "{labels}",
		Parameterized:    true,
		SupportsMultiple: true,
	}

	tests := []struct {
		name     string
		flag     string
		value    string
		values   []string
		expected string
		wantErr  bool
	}{
		{
			name:     "simple non-parameterized template",
			flag:     "simple",
			value:    "",
			values:   nil,
			expected: "simple:query",
			wantErr:  false,
		},
		{
			name:     "parameterized template with value",
			flag:     "author",
			value:    "dependabot",
			values:   nil,
			expected: "author:dependabot",
			wantErr:  false,
		},
		{
			name:     "labels template with single value",
			flag:     "labels",
			value:    "",
			values:   []string{"bug"},
			expected: "label:bug",
			wantErr:  false,
		},
		{
			name:     "labels template with multiple values",
			flag:     "labels",
			value:    "",
			values:   []string{"bug", "priority/high"},
			expected: "label:bug label:priority/high",
			wantErr:  false,
		},
		{
			name:     "labels template with negation",
			flag:     "labels",
			value:    "",
			values:   []string{"-hold", "bug"},
			expected: "-label:hold label:bug",
			wantErr:  false,
		},
		{
			name:     "labels template with all negations",
			flag:     "labels",
			value:    "",
			values:   []string{"-hold", "-wip"},
			expected: "-label:hold -label:wip",
			wantErr:  false,
		},
		{
			name:     "non-existent template",
			flag:     "nonexistent",
			value:    "",
			values:   nil,
			expected: "",
			wantErr:  true,
		},
		{
			name:     "parameterized template without query_template",
			flag:     "broken",
			value:    "test",
			values:   nil,
			expected: "",
			wantErr:  true,
		},
	}

	// Add a broken template for testing
	registry.templates["broken"] = QueryTemplate{
		Name:          "Broken",
		Flag:          "broken",
		Parameterized: true,
		QueryTemplate: "", // Missing query template
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result, err := registry.BuildQuery(tt.flag, tt.value, tt.values)

			if tt.wantErr {
				if err == nil {
					t.Errorf("BuildQuery() expected error, got nil")
				}
				return
			}

			if err != nil {
				t.Errorf("BuildQuery() unexpected error: %v", err)
				return
			}

			if result != tt.expected {
				t.Errorf("BuildQuery() = %q, want %q", result, tt.expected)
			}
		})
	}
}

func TestTemplateRegistry_GetTemplate(t *testing.T) {
	registry := &TemplateRegistry{
		templates: make(map[string]QueryTemplate),
	}

	template := QueryTemplate{
		Name: "Test",
		Flag: "test",
	}
	registry.templates["test"] = template

	// Test existing template
	result, exists := registry.GetTemplate("test")
	if !exists {
		t.Error("GetTemplate() should return true for existing template")
	}
	if result.Name != "Test" {
		t.Errorf("GetTemplate() returned wrong template: got %q, want %q", result.Name, "Test")
	}

	// Test non-existing template
	_, exists = registry.GetTemplate("nonexistent")
	if exists {
		t.Error("GetTemplate() should return false for non-existing template")
	}
}

func TestTemplateRegistry_GetAllTemplates(t *testing.T) {
	registry := &TemplateRegistry{
		templates: make(map[string]QueryTemplate),
	}

	template1 := QueryTemplate{Name: "Test1", Flag: "test1"}
	template2 := QueryTemplate{Name: "Test2", Flag: "test2"}

	registry.templates["test1"] = template1
	registry.templates["test2"] = template2

	all := registry.GetAllTemplates()
	if len(all) != 2 {
		t.Errorf("GetAllTemplates() returned %d templates, want 2", len(all))
	}

	if _, exists := all["test1"]; !exists {
		t.Error("GetAllTemplates() should include test1")
	}
	if _, exists := all["test2"]; !exists {
		t.Error("GetAllTemplates() should include test2")
	}
}

func TestTemplateRegistry_GetFlags(t *testing.T) {
	registry := &TemplateRegistry{
		templates: make(map[string]QueryTemplate),
	}

	registry.templates["zebra"] = QueryTemplate{Flag: "zebra"}
	registry.templates["alpha"] = QueryTemplate{Flag: "alpha"}
	registry.templates["beta"] = QueryTemplate{Flag: "beta"}

	flags := registry.GetFlags()
	expected := []string{"alpha", "beta", "zebra"}

	if len(flags) != len(expected) {
		t.Errorf("GetFlags() returned %d flags, want %d", len(flags), len(expected))
	}

	for i, flag := range flags {
		if flag != expected[i] {
			t.Errorf("GetFlags()[%d] = %q, want %q", i, flag, expected[i])
		}
	}
}

func TestTemplateRegistry_GetFlagsBySource(t *testing.T) {
	registry := &TemplateRegistry{
		templates: make(map[string]QueryTemplate),
	}

	registry.templates["embedded1"] = QueryTemplate{Flag: "embedded1", Source: "embedded"}
	registry.templates["user1"] = QueryTemplate{Flag: "user1", Source: "user"}
	registry.templates["embedded2"] = QueryTemplate{Flag: "embedded2", Source: "embedded"}

	embeddedFlags := registry.GetFlagsBySource("embedded")
	expected := []string{"embedded1", "embedded2"}

	if len(embeddedFlags) != len(expected) {
		t.Errorf("GetFlagsBySource('embedded') returned %d flags, want %d", len(embeddedFlags), len(expected))
	}

	for i, flag := range embeddedFlags {
		if flag != expected[i] {
			t.Errorf("GetFlagsBySource('embedded')[%d] = %q, want %q", i, flag, expected[i])
		}
	}

	userFlags := registry.GetFlagsBySource("user")
	if len(userFlags) != 1 || userFlags[0] != "user1" {
		t.Errorf("GetFlagsBySource('user') = %v, want [user1]", userFlags)
	}

	nonExistentFlags := registry.GetFlagsBySource("nonexistent")
	if len(nonExistentFlags) != 0 {
		t.Errorf("GetFlagsBySource('nonexistent') should return empty slice, got %v", nonExistentFlags)
	}
}

func TestTemplateRegistry_validateTemplate(t *testing.T) {
	registry := &TemplateRegistry{}

	tests := []struct {
		name     string
		template QueryTemplate
		wantErr  bool
		errMsg   string
	}{
		{
			name: "valid simple template",
			template: QueryTemplate{
				Name:        "Valid",
				Flag:        "valid",
				Description: "A valid template",
				Query:       "test:query",
			},
			wantErr: false,
		},
		{
			name: "valid parameterized template",
			template: QueryTemplate{
				Name:          "Valid Param",
				Flag:          "valid-param",
				Description:   "A valid parameterized template",
				QueryTemplate: "author:{value}",
				Parameterized: true,
			},
			wantErr: false,
		},
		{
			name: "missing name",
			template: QueryTemplate{
				Flag:        "test",
				Description: "Test",
				Query:       "test:query",
			},
			wantErr: true,
			errMsg:  "template name is required",
		},
		{
			name: "missing flag",
			template: QueryTemplate{
				Name:        "Test",
				Description: "Test",
				Query:       "test:query",
			},
			wantErr: true,
			errMsg:  "template flag is required",
		},
		{
			name: "missing description",
			template: QueryTemplate{
				Name:  "Test",
				Flag:  "test",
				Query: "test:query",
			},
			wantErr: true,
			errMsg:  "template description is required",
		},
		{
			name: "missing query and query_template",
			template: QueryTemplate{
				Name:        "Test",
				Flag:        "test",
				Description: "Test",
			},
			wantErr: true,
			errMsg:  "either query or query_template is required",
		},
		{
			name: "both query and query_template",
			template: QueryTemplate{
				Name:          "Test",
				Flag:          "test",
				Description:   "Test",
				Query:         "test:query",
				QueryTemplate: "test:{value}",
			},
			wantErr: true,
			errMsg:  "only one of query or query_template should be specified",
		},
		{
			name: "parameterized without query_template",
			template: QueryTemplate{
				Name:          "Test",
				Flag:          "test",
				Description:   "Test",
				Parameterized: true,
				Query:         "test:query",
			},
			wantErr: true,
			errMsg:  "parameterized templates must have query_template",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := registry.validateTemplate(tt.template)

			if tt.wantErr {
				if err == nil {
					t.Error("validateTemplate() expected error, got nil")
					return
				}
				if err.Error() != tt.errMsg {
					t.Errorf("validateTemplate() error = %q, want %q", err.Error(), tt.errMsg)
				}
			} else {
				if err != nil {
					t.Errorf("validateTemplate() unexpected error: %v", err)
				}
			}
		})
	}
}

func TestNewTemplateRegistry(t *testing.T) {
	// This test will load the actual embedded templates
	registry, err := NewTemplateRegistry()
	if err != nil {
		t.Fatalf("NewTemplateRegistry() error = %v", err)
	}

	if registry == nil {
		t.Fatal("NewTemplateRegistry() returned nil registry")
	}

	// Check that some embedded templates are loaded
	templates := registry.GetAllTemplates()
	if len(templates) == 0 {
		t.Error("NewTemplateRegistry() should load embedded templates")
	}

	// Check for some expected templates
	expectedFlags := []string{"needs-approve", "needs-lgtm", "failing-ci", "author", "label"}
	for _, flag := range expectedFlags {
		if _, exists := registry.GetTemplate(flag); !exists {
			t.Errorf("Expected embedded template %q not found", flag)
		}
	}
}

// Test helper function to create a temporary directory with test templates
func createTestTemplateDir(t *testing.T) string {
	tmpDir := t.TempDir()
	templatesDir := filepath.Join(tmpDir, ".config", "autoprat", "templates")

	err := os.MkdirAll(templatesDir, 0755)
	if err != nil {
		t.Fatal(err)
	}

	// Create a valid test template
	validTemplate := `name: "User Test Template"
flag: "user-test"
description: "A user-defined test template"
query: "user:test"`

	err = os.WriteFile(filepath.Join(templatesDir, "user-test.yaml"), []byte(validTemplate), 0644)
	if err != nil {
		t.Fatal(err)
	}

	// Create an invalid template to test error handling
	invalidTemplate := `name: "Invalid Template"
flag: "invalid"
# missing description and query`

	err = os.WriteFile(filepath.Join(templatesDir, "invalid.yaml"), []byte(invalidTemplate), 0644)
	if err != nil {
		t.Fatal(err)
	}

	return tmpDir
}

func TestTemplateRegistry_loadUserTemplates(t *testing.T) {
	// Save original HOME
	originalHome := os.Getenv("HOME")
	defer os.Setenv("HOME", originalHome)

	// Create test directory with only valid template
	tmpDir := t.TempDir()
	templatesDir := filepath.Join(tmpDir, ".config", "autoprat", "templates")

	err := os.MkdirAll(templatesDir, 0755)
	if err != nil {
		t.Fatal(err)
	}

	// Create a valid test template
	validTemplate := `name: "User Test Template"
flag: "user-test"
description: "A user-defined test template"
query: "user:test"`

	err = os.WriteFile(filepath.Join(templatesDir, "user-test.yaml"), []byte(validTemplate), 0644)
	if err != nil {
		t.Fatal(err)
	}

	os.Setenv("HOME", tmpDir)

	registry := &TemplateRegistry{
		templates: make(map[string]QueryTemplate),
	}

	// This should load user templates successfully
	err = registry.loadUserTemplates()
	if err != nil {
		t.Errorf("loadUserTemplates() unexpected error: %v", err)
	}

	// Should load the valid template
	template, exists := registry.GetTemplate("user-test")
	if !exists {
		t.Error("loadUserTemplates() should load valid user template")
	} else {
		if template.Source != "user" {
			t.Errorf("User template source = %q, want 'user'", template.Source)
		}
		if template.Name != "User Test Template" {
			t.Errorf("User template name = %q, want 'User Test Template'", template.Name)
		}
	}
}

func TestTemplateRegistry_loadUserTemplates_InvalidTemplate(t *testing.T) {
	// Save original HOME
	originalHome := os.Getenv("HOME")
	defer os.Setenv("HOME", originalHome)

	// Create test directory with invalid template
	tmpDir := t.TempDir()
	templatesDir := filepath.Join(tmpDir, ".config", "autoprat", "templates")

	err := os.MkdirAll(templatesDir, 0755)
	if err != nil {
		t.Fatal(err)
	}

	// Create an invalid template to test error handling
	invalidTemplate := `name: "Invalid Template"
flag: "invalid"
# missing description and query`

	err = os.WriteFile(filepath.Join(templatesDir, "invalid.yaml"), []byte(invalidTemplate), 0644)
	if err != nil {
		t.Fatal(err)
	}

	os.Setenv("HOME", tmpDir)

	registry := &TemplateRegistry{
		templates: make(map[string]QueryTemplate),
	}

	// This should fail due to invalid template
	err = registry.loadUserTemplates()
	if err == nil {
		t.Error("loadUserTemplates() should return error for invalid template")
	}
}

func TestTemplateRegistry_loadUserTemplates_NoDirectory(t *testing.T) {
	// Save original HOME
	originalHome := os.Getenv("HOME")
	defer os.Setenv("HOME", originalHome)

	// Set HOME to a non-existent directory
	os.Setenv("HOME", "/non/existent/path")

	registry := &TemplateRegistry{
		templates: make(map[string]QueryTemplate),
	}

	// Should not fail when directory doesn't exist
	err := registry.loadUserTemplates()
	if err != nil {
		t.Errorf("loadUserTemplates() should not fail when directory doesn't exist, got error: %v", err)
	}
}

func TestTemplateRegistry_loadUserTemplates_NonYamlFiles(t *testing.T) {
	// Save original HOME
	originalHome := os.Getenv("HOME")
	defer os.Setenv("HOME", originalHome)

	// Create test directory with non-YAML files
	tmpDir := t.TempDir()
	templatesDir := filepath.Join(tmpDir, ".config", "autoprat", "templates")

	err := os.MkdirAll(templatesDir, 0755)
	if err != nil {
		t.Fatal(err)
	}

	// Create a non-YAML file (should be ignored)
	err = os.WriteFile(filepath.Join(templatesDir, "not-yaml.txt"), []byte("not yaml"), 0644)
	if err != nil {
		t.Fatal(err)
	}

	// Create a valid YAML template
	validTemplate := `name: "Valid Template"
flag: "valid"
description: "A valid template"
query: "valid:query"`

	err = os.WriteFile(filepath.Join(templatesDir, "valid.yaml"), []byte(validTemplate), 0644)
	if err != nil {
		t.Fatal(err)
	}

	os.Setenv("HOME", tmpDir)

	registry := &TemplateRegistry{
		templates: make(map[string]QueryTemplate),
	}

	// Should load only YAML files
	err = registry.loadUserTemplates()
	if err != nil {
		t.Errorf("loadUserTemplates() unexpected error: %v", err)
	}

	// Should have loaded only the YAML template
	templates := registry.GetAllTemplates()
	if len(templates) != 1 {
		t.Errorf("Expected 1 template, got %d", len(templates))
	}

	if _, exists := registry.GetTemplate("valid"); !exists {
		t.Error("Should have loaded valid template")
	}
}

func TestNewTemplateRegistry_ErrorPath(t *testing.T) {
	// This is tricky to test since we can't easily break the embedded filesystem
	// But we can test that NewTemplateRegistry handles user template errors gracefully

	// Save original HOME
	originalHome := os.Getenv("HOME")
	defer os.Setenv("HOME", originalHome)

	// Create a directory with invalid permissions to cause loadUserTemplates to fail
	tmpDir := t.TempDir()
	templatesDir := filepath.Join(tmpDir, ".config", "autoprat", "templates")

	err := os.MkdirAll(templatesDir, 0755)
	if err != nil {
		t.Fatal(err)
	}

	// Create an invalid template
	invalidTemplate := `invalid yaml content:`
	err = os.WriteFile(filepath.Join(templatesDir, "invalid.yaml"), []byte(invalidTemplate), 0644)
	if err != nil {
		t.Fatal(err)
	}

	os.Setenv("HOME", tmpDir)

	// Should still succeed despite user template error (it just warns)
	registry, err := NewTemplateRegistry()
	if err != nil {
		t.Errorf("NewTemplateRegistry() should not fail due to user template errors, got: %v", err)
	}

	if registry == nil {
		t.Error("NewTemplateRegistry() should return registry even with user template errors")
	}

	// Should still have embedded templates
	templates := registry.GetAllTemplates()
	if len(templates) == 0 {
		t.Error("Should still have embedded templates despite user template errors")
	}
}

func TestBuildQuery_EdgeCases(t *testing.T) {
	registry := &TemplateRegistry{
		templates: make(map[string]QueryTemplate),
	}

	// Template that uses both {value} and {labels} (edge case)
	registry.templates["complex"] = QueryTemplate{
		Name:             "Complex",
		Flag:             "complex",
		QueryTemplate:    "author:{value} {labels}",
		Parameterized:    true,
		SupportsMultiple: true,
	}

	// Template with empty query template
	registry.templates["empty-template"] = QueryTemplate{
		Name:          "Empty Template",
		Flag:          "empty-template",
		QueryTemplate: "",
		Parameterized: true,
	}

	tests := []struct {
		name     string
		flag     string
		value    string
		values   []string
		expected string
		wantErr  bool
	}{
		{
			name:     "complex template with value and labels",
			flag:     "complex",
			value:    "user",
			values:   []string{"bug", "-hold"},
			expected: "author:user label:bug -label:hold",
			wantErr:  false,
		},
		{
			name:     "template with no substitutions",
			flag:     "complex",
			value:    "",
			values:   []string{},
			expected: "author: ",
			wantErr:  false,
		},
		{
			name:     "empty labels array",
			flag:     "complex",
			value:    "user",
			values:   []string{},
			expected: "author:user ",
			wantErr:  false,
		},
		{
			name:    "empty query template should error",
			flag:    "empty-template",
			value:   "test",
			wantErr: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result, err := registry.BuildQuery(tt.flag, tt.value, tt.values)

			if tt.wantErr {
				if err == nil {
					t.Error("BuildQuery() expected error, got nil")
				}
				return
			}

			if err != nil {
				t.Errorf("BuildQuery() unexpected error: %v", err)
				return
			}

			if result != tt.expected {
				t.Errorf("BuildQuery() = %q, want %q", result, tt.expected)
			}
		})
	}
}
