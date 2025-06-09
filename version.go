package main

import (
	"runtime"
	"runtime/debug"
)

// Info contains version and build information.
type Info struct {
	Version   string
	BuildTime string
	GoVersion string
	Platform  string
}

// Get returns the current version information.
func Get() Info {
	buildVersion := "unknown"
	buildTime := "unknown"
	goVer := runtime.Version()

	if info, ok := debug.ReadBuildInfo(); ok {
		if info.Main.Version != "(devel)" && info.Main.Version != "" {
			buildVersion = info.Main.Version
		}

		for _, setting := range info.Settings {
			switch setting.Key {
			case "vcs.time":
				buildTime = setting.Value
			}
		}
	}

	return Info{
		Version:   buildVersion,
		BuildTime: buildTime,
		GoVersion: goVer,
		Platform:  runtime.GOOS + "/" + runtime.GOARCH,
	}
}
