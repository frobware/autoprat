package search

import (
	"embed"
	"fmt"
	"os"
	"path/filepath"
	"slices"
	"strings"

	"gopkg.in/yaml.v3"
)

//go:embed templates/embedded/*.yaml
var embeddedTemplates embed.FS

// QueryTemplate represents a query template loaded from YAML.
type QueryTemplate struct {
	Name             string `yaml:"name"`
	Flag             string `yaml:"flag"`
	FlagShort        string `yaml:"flag_short,omitempty"`
	Description      string `yaml:"description"`
	Query            string `yaml:"query,omitempty"`
	QueryTemplate    string `yaml:"query_template,omitempty"`
	Parameterized    bool   `yaml:"parameterized,omitempty"`
	SupportsMultiple bool   `yaml:"supports_multiple,omitempty"`
	Source           string `yaml:"-"` // "embedded" or "user"
}

// TemplateRegistry holds all available query templates.
type TemplateRegistry struct {
	templates map[string]QueryTemplate
}

// NewTemplateRegistry creates a new template registry and loads all templates.
func NewTemplateRegistry() (*TemplateRegistry, error) {
	r := &TemplateRegistry{
		templates: make(map[string]QueryTemplate),
	}

	if err := r.loadEmbeddedTemplates(); err != nil {
		return nil, fmt.Errorf("failed to load embedded templates: %w", err)
	}

	if err := r.loadUserTemplates(); err != nil {
		fmt.Fprintf(os.Stderr, "Warning: failed to load user templates: %v\n", err)
	}

	return r, nil
}

// loadEmbeddedTemplates loads templates from the embedded filesystem.
func (r *TemplateRegistry) loadEmbeddedTemplates() error {
	entries, err := embeddedTemplates.ReadDir("templates/embedded")
	if err != nil {
		return fmt.Errorf("failed to read embedded templates directory: %w", err)
	}

	for _, entry := range entries {
		if !strings.HasSuffix(entry.Name(), ".yaml") {
			continue
		}

		content, err := embeddedTemplates.ReadFile("templates/embedded/" + entry.Name())
		if err != nil {
			return fmt.Errorf("failed to read embedded template file %s: %w", entry.Name(), err)
		}

		var template QueryTemplate
		if err := yaml.Unmarshal(content, &template); err != nil {
			return fmt.Errorf("failed to parse embedded template file %s: %w", entry.Name(), err)
		}

		if err := r.validateTemplate(template); err != nil {
			return fmt.Errorf("invalid embedded template %s: %w", entry.Name(), err)
		}

		template.Source = "embedded"
		r.templates[template.Flag] = template
	}

	return nil
}

// loadUserTemplates loads templates from the user's config directory.
func (r *TemplateRegistry) loadUserTemplates() error {
	homeDir, err := os.UserHomeDir()
	if err != nil {
		return fmt.Errorf("failed to get user home directory: %w", err)
	}

	templatesDir := filepath.Join(homeDir, ".config", "autoprat", "templates")

	if _, err := os.Stat(templatesDir); os.IsNotExist(err) {
		return nil
	}

	entries, err := os.ReadDir(templatesDir)
	if err != nil {
		return fmt.Errorf("failed to read user templates directory: %w", err)
	}

	for _, entry := range entries {
		if !strings.HasSuffix(entry.Name(), ".yaml") {
			continue
		}

		content, err := os.ReadFile(filepath.Join(templatesDir, entry.Name()))
		if err != nil {
			return fmt.Errorf("failed to read user template file %s: %w", entry.Name(), err)
		}

		var template QueryTemplate
		if err := yaml.Unmarshal(content, &template); err != nil {
			return fmt.Errorf("failed to parse user template file %s: %w", entry.Name(), err)
		}

		if err := r.validateTemplate(template); err != nil {
			return fmt.Errorf("invalid user template %s: %w", entry.Name(), err)
		}

		template.Source = "user"
		r.templates[template.Flag] = template
	}

	return nil
}

// validateTemplate ensures a template definition is valid.
func (r *TemplateRegistry) validateTemplate(template QueryTemplate) error {
	if template.Name == "" {
		return fmt.Errorf("template name is required")
	}
	if template.Flag == "" {
		return fmt.Errorf("template flag is required")
	}
	if template.Description == "" {
		return fmt.Errorf("template description is required")
	}
	if template.Query == "" && template.QueryTemplate == "" {
		return fmt.Errorf("either query or query_template is required")
	}
	if template.Query != "" && template.QueryTemplate != "" {
		return fmt.Errorf("only one of query or query_template should be specified")
	}
	if template.Parameterized && template.QueryTemplate == "" {
		return fmt.Errorf("parameterized templates must have query_template")
	}

	return nil
}

// GetTemplate returns the template for the given flag name.
func (r *TemplateRegistry) GetTemplate(flag string) (QueryTemplate, bool) {
	template, exists := r.templates[flag]
	return template, exists
}

// GetAllTemplates returns all loaded templates.
func (r *TemplateRegistry) GetAllTemplates() map[string]QueryTemplate {
	return r.templates
}

// GetFlags returns all available template flag names in sorted order.
func (r *TemplateRegistry) GetFlags() []string {
	var flags []string
	for flag := range r.templates {
		flags = append(flags, flag)
	}

	slices.Sort(flags)
	return flags
}

// GetFlagsBySource returns template flag names for a specific source, sorted.
func (r *TemplateRegistry) GetFlagsBySource(source string) []string {
	var flags []string
	for flag, template := range r.templates {
		if template.Source == source {
			flags = append(flags, flag)
		}
	}

	slices.Sort(flags)
	return flags
}

// BuildQuery builds a search query from a template with the given parameters.
func (r *TemplateRegistry) BuildQuery(flag string, value string, values []string) (string, error) {
	template, exists := r.GetTemplate(flag)
	if !exists {
		return "", fmt.Errorf("template %s not found", flag)
	}

	// Non-parameterized templates
	if !template.Parameterized {
		return template.Query, nil
	}

	// Parameterized templates
	if template.QueryTemplate == "" {
		return "", fmt.Errorf("parameterized template %s missing query_template", flag)
	}

	query := template.QueryTemplate

	// Handle single value substitution
	if strings.Contains(query, "{value}") {
		query = strings.ReplaceAll(query, "{value}", value)
	}

	// Handle multi-value substitution (for labels)
	if strings.Contains(query, "{labels}") {
		var labelTerms []string
		for _, label := range values {
			if strings.HasPrefix(label, "-") {
				labelName := strings.TrimPrefix(label, "-")
				labelTerms = append(labelTerms, fmt.Sprintf("-label:%s", labelName))
			} else {
				labelTerms = append(labelTerms, fmt.Sprintf("label:%s", label))
			}
		}
		query = strings.ReplaceAll(query, "{labels}", strings.Join(labelTerms, " "))
	}

	return query, nil
}
