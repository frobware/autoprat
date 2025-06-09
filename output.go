package main

import (
	"fmt"
)

// CommandFormatter outputs commands as-is.
type CommandFormatter struct{}

// Format outputs commands for execution.
func (f *CommandFormatter) Format(result Result, config *Config) error {
	cmdResult, ok := result.(CommandResult)
	if !ok {
		return fmt.Errorf("CommandFormatter expects CommandResult, got %T", result)
	}

	for _, cmd := range cmdResult.Commands {
		fmt.Println(cmd)
	}
	return nil
}

// FormatResult determines the appropriate formatter based on the result type and config.
func FormatResult(result Result, config *Config) error {
	switch r := result.(type) {
	case CommandResult:
		formatter := &CommandFormatter{}
		return formatter.Format(result, config)
	case PRResult:
		var formatter Formatter
		if config.Detailed || config.DetailedWithLogs {
			formatter = &VerboseFormatter{}
		} else if config.Quiet {
			formatter = &QuietFormatter{}
		} else {
			formatter = &TabularFormatter{}
		}
		return formatter.Format(result, config)
	default:
		return fmt.Errorf("unknown result type: %T", r)
	}
}
