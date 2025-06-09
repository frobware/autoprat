package main

import (
	"time"
)

// Config holds all configuration and arguments for the application.
type Config struct {
	Repositories []string
	ParsedPRs    []PullRequestRef
	Actions      []Action
	SearchQuery  string
	// Runtime flags
	Throttle         time.Duration
	DebugMode        bool
	Detailed         bool
	DetailedWithLogs bool
	Quiet            bool
}
