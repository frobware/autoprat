package main

import (
	"fmt"
	"os"

	"github.com/spf13/pflag"
)

// Parse parses command-line arguments and returns a populated Config.
// This is the main entry point for CLI argument processing.
// Action and template definitions must be injected to maintain separation of concerns.
func Parse(availableActions map[string]ActionDefinition, availableTemplates map[string]QueryTemplate) (*Config, error) {

	// Setup all flags
	flagCategories, flagRefs := SetupFlags(availableActions, availableTemplates)

	// Parse command line
	pflag.Parse()

	// Handle version flag before validation
	showVersion := flagRefs["version"].(*bool)
	if *showVersion {
		info := Get()
		fmt.Printf("autoprat version %s\n", info.Version)
		fmt.Printf("Built: %s\n", info.BuildTime)
		fmt.Printf("Go version: %s\n", info.GoVersion)
		fmt.Printf("Platform: %s\n", info.Platform)
		os.Exit(0)
	}

	// Build flag maps for parsing
	actionFlags, templateFlags, parameterisedTemplateFlags := BuildFlagMapsForParsing(flagCategories, flagRefs)

	// Parse and validate arguments
	config, err := parseAndValidateArgs(availableActions, actionFlags, availableTemplates, templateFlags, parameterisedTemplateFlags, flagRefs)
	if err != nil {
		pflag.Usage()
		fmt.Fprintf(os.Stderr, "\nError: %v\n", err)
		os.Exit(1)
	}

	return config, nil
}
