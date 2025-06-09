package main

import (
	"runtime"
	"strings"
	"testing"
)

func TestGet(t *testing.T) {
	info := Get()

	// Go's build information always populates something
	if info.Version == "" {
		t.Error("Version should not be empty")
	}

	if info.BuildTime == "" {
		t.Error("BuildTime should not be empty")
	}

	if info.GoVersion == "" {
		t.Error("GoVersion should not be empty")
	}

	if info.Platform == "" {
		t.Error("Platform should not be empty")
	}

	// Test GoVersion format
	if !strings.HasPrefix(info.GoVersion, "go") {
		t.Errorf("GoVersion should start with 'go', got: %s", info.GoVersion)
	}

	// Test Platform format (should be os/arch)
	platformParts := strings.Split(info.Platform, "/")
	if len(platformParts) != 2 {
		t.Errorf("Platform should be in format 'os/arch', got: %s", info.Platform)
	}

	// Verify platform matches runtime values
	expectedPlatform := runtime.GOOS + "/" + runtime.GOARCH
	if info.Platform != expectedPlatform {
		t.Errorf("Platform should be %s, got: %s", expectedPlatform, info.Platform)
	}

	// Verify GoVersion matches runtime
	if info.GoVersion != runtime.Version() {
		t.Errorf("GoVersion should be %s, got: %s", runtime.Version(), info.GoVersion)
	}
}

func TestInfoStruct(t *testing.T) {
	// Test that Info struct can be constructed manually
	info := Info{
		Version:   "v1.0.0",
		BuildTime: "2024-01-01T00:00:00Z",
		GoVersion: "go1.21.0",
		Platform:  "linux/amd64",
	}

	if info.Version != "v1.0.0" {
		t.Errorf("Expected Version v1.0.0, got %s", info.Version)
	}

	if info.BuildTime != "2024-01-01T00:00:00Z" {
		t.Errorf("Expected BuildTime 2024-01-01T00:00:00Z, got %s", info.BuildTime)
	}

	if info.GoVersion != "go1.21.0" {
		t.Errorf("Expected GoVersion go1.21.0, got %s", info.GoVersion)
	}

	if info.Platform != "linux/amd64" {
		t.Errorf("Expected Platform linux/amd64, got %s", info.Platform)
	}
}

func TestGetConsistency(t *testing.T) {
	// Test that multiple calls return consistent results
	info1 := Get()
	info2 := Get()

	if info1.Version != info2.Version {
		t.Error("Version should be consistent across multiple calls")
	}

	if info1.BuildTime != info2.BuildTime {
		t.Error("BuildTime should be consistent across multiple calls")
	}

	if info1.GoVersion != info2.GoVersion {
		t.Error("GoVersion should be consistent across multiple calls")
	}

	if info1.Platform != info2.Platform {
		t.Error("Platform should be consistent across multiple calls")
	}
}
