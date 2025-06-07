package main

import (
	"fmt"
	"net/url"
	"os"
	"regexp"
	"slices"
	"sort"
	"strconv"
	"strings"
	"time"

	"github.com/spf13/pflag"

	"github.com/frobware/autoprat/github/actions"
	"github.com/frobware/autoprat/github/filters"
	"github.com/frobware/autoprat/github/search"
)

// pullRequestRefRegex matches GitHub PR URLs like "/owner/repo/pull/123" or "/owner/repo/pull/123/".
var pullRequestRefRegex = regexp.MustCompile(`^/([^/]+)/([^/]+)/pull/(\d+)/?$`)

// FlagInfo contains information about a command-line flag.
type FlagInfo struct {
	Name        string
	ShortName   string
	Type        string // "bool", "string", "stringSlice", "duration"
	Description string
	Default     interface{}
}

// Display formats the flag for display in help text.
func (flag FlagInfo) Display() string {
	display := "--" + flag.Name

	// Add short flag if available.
	if flag.ShortName != "" {
		display = fmt.Sprintf("-%s, --%s", flag.ShortName, flag.Name)
	}

	// Add type indicator for non-boolean flags.
	switch flag.Type {
	case "string":
		display += " string"
	case "stringSlice":
		display += " strings"
	case "duration":
		display += " duration"
	}

	return display
}

// FlagCategory represents a group of related flags.
type FlagCategory struct {
	Name  string
	Flags []FlagInfo
}

// DefineAllFlags creates all command-line flags and returns categorised flag information.
func DefineAllFlags(actionRegistry *actions.Registry, templateRegistry *search.TemplateRegistry) []FlagCategory {
	categories := []FlagCategory{
		{
			Name: "Repository:",
			Flags: []FlagInfo{
				{Name: "repo", ShortName: "r", Type: "string", Description: "GitHub repo (owner/repo)", Default: ""},
			},
		},
		{
			Name: "Output:",
			Flags: []FlagInfo{
				{Name: "verbose", ShortName: "v", Type: "bool", Description: "Print PR status only", Default: false},
				{Name: "verbose-verbose", ShortName: "V", Type: "bool", Description: "Print PR status with error logs from failing checks", Default: false},
				{Name: "quiet", ShortName: "q", Type: "bool", Description: "Print PR numbers only", Default: false},
			},
		},
		{
			Name: "Utility:",
			Flags: []FlagInfo{
				{Name: "debug", ShortName: "", Type: "bool", Description: "Enable debug logging", Default: false},
				{Name: "version", ShortName: "", Type: "bool", Description: "Show version information", Default: false},
			},
		},
	}

	// Add filters from template registry
	var filterFlags []FlagInfo
	for flag, template := range templateRegistry.GetAllTemplates() {
		flagType := "bool"
		var defaultVal interface{} = false
		if template.Parameterized {
			if template.SupportsMultiple {
				flagType = "stringSlice"
				defaultVal = []string{}
			} else {
				flagType = "string"
				defaultVal = ""
			}
		}
		filterFlags = append(filterFlags, FlagInfo{
			Name:        flag,
			ShortName:   template.FlagShort,
			Type:        flagType,
			Description: template.Description,
			Default:     defaultVal,
		})
	}
	// Sort filter flags for consistent output
	slices.SortFunc(filterFlags, func(a, b FlagInfo) int {
		return strings.Compare(a.Name, b.Name)
	})
	categories = append(categories[:1], append([]FlagCategory{{Name: "Filters:", Flags: filterFlags}}, categories[1:]...)...)

	// Add actions from action registry plus comment/throttle flags
	var actionFlags []FlagInfo

	// Add comment and throttle flags first
	actionFlags = append(actionFlags, FlagInfo{
		Name:        "comment",
		ShortName:   "c",
		Type:        "stringSlice",
		Description: "Generate comment commands",
		Default:     []string{},
	})
	actionFlags = append(actionFlags, FlagInfo{
		Name:        "throttle",
		ShortName:   "",
		Type:        "duration",
		Description: "Throttle identical comments to limit posting frequency",
		Default:     time.Duration(0),
	})

	// Add dynamic actions from registry
	for flag, action := range actionRegistry.GetAllActions() {
		actionFlags = append(actionFlags, FlagInfo{
			Name:        flag,
			ShortName:   "",
			Type:        "bool",
			Description: action.Description,
			Default:     false,
		})
	}

	// Sort action flags for consistent output
	slices.SortFunc(actionFlags, func(a, b FlagInfo) int {
		return strings.Compare(a.Name, b.Name)
	})
	// Insert actions before "Output:" section
	actionCategory := FlagCategory{Name: "Actions:", Flags: actionFlags}
	// Find where to insert (after Filters, before Output)
	insertIdx := 2 // After Repository and Filters
	categories = append(categories[:insertIdx], append([]FlagCategory{actionCategory}, categories[insertIdx:]...)...)

	return categories
}

// registerFlags registers all flags with pflag and returns references to them.
func registerFlags(categories []FlagCategory) map[string]interface{} {
	flagRefs := make(map[string]interface{})

	for _, category := range categories {
		for _, flag := range category.Flags {
			switch flag.Type {
			case "bool":
				if flag.ShortName != "" {
					flagRefs[flag.Name] = pflag.BoolP(flag.Name, flag.ShortName, flag.Default.(bool), flag.Description)
				} else {
					flagRefs[flag.Name] = pflag.Bool(flag.Name, flag.Default.(bool), flag.Description)
				}
			case "string":
				if flag.ShortName != "" {
					flagRefs[flag.Name] = pflag.StringP(flag.Name, flag.ShortName, flag.Default.(string), flag.Description)
				} else {
					flagRefs[flag.Name] = pflag.String(flag.Name, flag.Default.(string), flag.Description)
				}
			case "stringSlice":
				if flag.ShortName != "" {
					flagRefs[flag.Name] = pflag.StringSliceP(flag.Name, flag.ShortName, flag.Default.([]string), flag.Description)
				} else {
					flagRefs[flag.Name] = pflag.StringSlice(flag.Name, flag.Default.([]string), flag.Description)
				}
			case "duration":
				flagRefs[flag.Name] = pflag.Duration(flag.Name, flag.Default.(time.Duration), flag.Description)
			}
		}
	}

	return flagRefs
}

// SetupFlags initialises all flags and returns the categories and flag references.
func SetupFlags(actionRegistry *actions.Registry, templateRegistry *search.TemplateRegistry) ([]FlagCategory, map[string]interface{}) {
	// Define all flags and get their categories
	flagCategories := DefineAllFlags(actionRegistry, templateRegistry)

	// Register all flags and get references to them
	flagRefs := registerFlags(flagCategories)

	// Set up help function
	pflag.Usage = func() {
		PrintHelpFromFlags(os.Args[0], flagCategories)
	}

	return flagCategories, flagRefs
}

// BuildFlagMapsForParsing builds the flag maps needed by parseAndValidateArgs.
func BuildFlagMapsForParsing(flagCategories []FlagCategory, flagRefs map[string]interface{}) (map[string]*bool, map[string]*bool, map[string]interface{}) {
	actionFlags := make(map[string]*bool)
	templateFlags := make(map[string]*bool)
	parameterisedTemplateFlags := make(map[string]interface{})

	// Populate flag maps from flagRefs
	for _, category := range flagCategories {
		if category.Name == "Actions:" {
			for _, flag := range category.Flags {
				// Only bool flags from actions go in actionFlags (excludes comment/throttle)
				if flag.Type == "bool" {
					actionFlags[flag.Name] = flagRefs[flag.Name].(*bool)
				}
			}
		} else if category.Name == "Filters:" {
			for _, flag := range category.Flags {
				switch flag.Type {
				case "bool":
					templateFlags[flag.Name] = flagRefs[flag.Name].(*bool)
				case "string":
					parameterisedTemplateFlags[flag.Name] = flagRefs[flag.Name].(*string)
				case "stringSlice":
					parameterisedTemplateFlags[flag.Name] = flagRefs[flag.Name].(*[]string)
				}
			}
		}
	}

	return actionFlags, templateFlags, parameterisedTemplateFlags
}

// PullRequestRef represents a parsed PR argument with number and optional repository.
type PullRequestRef struct {
	Number int
	Repo   string // Empty for numeric arguments, populated for URLs.
}

// parsePRArgument extracts a PR number and repository from either a numeric string or GitHub URL.
// For URLs, extracts both the repository and PR number.
// For numeric arguments, only returns the PR number.
// Supports formats:
//   - "123" (numeric PR number)
//   - "https://github.com/owner/repo/pull/123" (GitHub PR URL)
func parsePRArgument(arg string) (PullRequestRef, error) {
	if num, err := strconv.Atoi(arg); err == nil {
		return PullRequestRef{Number: num}, nil
	}

	parsedURL, err := url.Parse(arg)
	if err != nil {
		return PullRequestRef{}, fmt.Errorf("invalid PR number or URL %q", arg)
	}

	matches := pullRequestRefRegex.FindStringSubmatch(parsedURL.Path)
	if len(matches) != 4 {
		return PullRequestRef{}, fmt.Errorf("invalid GitHub PR URL %q", arg)
	}

	urlRepo := matches[1] + "/" + matches[2]
	prNumber, err := strconv.Atoi(matches[3])
	if err != nil {
		return PullRequestRef{}, fmt.Errorf("invalid PR number in URL %q", arg)
	}

	return PullRequestRef{Number: prNumber, Repo: urlRepo}, nil
}

// Config holds all configuration and arguments for the application.
type Config struct {
	Repositories []string
	ParsedPRs    []PullRequestRef
	Actions      []actions.Action
	Filters      []filters.FilterDefinition
	SearchQuery  string
	// Runtime flags
	Throttle       time.Duration
	DebugMode      bool
	Verbose        bool
	VerboseVerbose bool
	Quiet          bool
}

// parseAndValidateArgs parses command line arguments and validates
// repository requirements.
func parseAndValidateArgs(actionRegistry *actions.Registry, actionFlags map[string]*bool, templateRegistry *search.TemplateRegistry, templateFlags map[string]*bool, parameterisedTemplateFlags map[string]interface{}, flagRefs map[string]interface{}) (*Config, error) {
	// Extract runtime flags
	repo := flagRefs["repo"].(*string)
	comment := flagRefs["comment"].(*[]string)
	throttle := flagRefs["throttle"].(*time.Duration)
	debugMode := flagRefs["debug"].(*bool)
	verbose := flagRefs["verbose"].(*bool)
	verboseVerbose := flagRefs["verbose-verbose"].(*bool)
	quiet := flagRefs["quiet"].(*bool)
	prNumbers := pflag.Args()

	var parsedPRs []PullRequestRef
	repositories := make(map[string]bool)
	hasNumericArgs := false

	for _, s := range prNumbers {
		prArg, err := parsePRArgument(s)
		if err != nil {
			return nil, err
		}
		parsedPRs = append(parsedPRs, prArg)

		if prArg.Repo == "" {
			hasNumericArgs = true
		} else {
			repositories[prArg.Repo] = true
		}
	}

	if *repo != "" {
		repositories[*repo] = true
	}

	if len(repositories) == 0 && (hasNumericArgs || len(prNumbers) == 0) {
		return nil, fmt.Errorf("--repo is required when using numeric PR arguments or no PR arguments")
	}

	var repoList []string
	for repo := range repositories {
		repoList = append(repoList, repo)
	}
	sort.Strings(repoList)

	var allActions []actions.Action
	for _, c := range *comment {
		allActions = append(allActions, actions.Action{
			Comment:   c,
			Predicate: actions.PredicateNone,
		})
	}

	for flag, flagPtr := range actionFlags {
		if *flagPtr {
			actionDef, exists := actionRegistry.GetAction(flag)
			if exists {
				allActions = append(allActions, actionDef.ToAction())
			}
		}
	}

	// Build search query from templates
	var queryTerms []string

	// Handle boolean templates (non-parameterised).
	for flag, flagPtr := range templateFlags {
		if *flagPtr {
			template, exists := templateRegistry.GetTemplate(flag)
			if exists && !template.Parameterized {
				queryTerms = append(queryTerms, template.Query)
			}
		}
	}

	// Handle parameterised templates.
	for flag, flagPtr := range parameterisedTemplateFlags {
		_, exists := templateRegistry.GetTemplate(flag)
		if !exists {
			continue
		}

		var query string
		var queryErr error

		if stringPtr, ok := flagPtr.(*string); ok && *stringPtr != "" {
			query, queryErr = templateRegistry.BuildQuery(flag, *stringPtr, nil)
		} else if slicePtr, ok := flagPtr.(*[]string); ok && len(*slicePtr) > 0 {
			query, queryErr = templateRegistry.BuildQuery(flag, "", *slicePtr)
		}

		if queryErr == nil && query != "" {
			queryTerms = append(queryTerms, query)
		}
	}

	searchQuery := strings.Join(queryTerms, " ")

	return &Config{
		Repositories:   repoList,
		ParsedPRs:      parsedPRs,
		Actions:        allActions,
		SearchQuery:    searchQuery,
		Throttle:       *throttle,
		DebugMode:      *debugMode,
		Verbose:        *verbose,
		VerboseVerbose: *verboseVerbose,
		Quiet:          *quiet,
	}, nil
}
