//! Build script for autoprat - generates version information.
//!
//! ## Version Generation Algorithm
//!
//! This build script generates version strings using the following algorithm:
//!
//! 1. Try `git describe --tags --always --dirty`
//!    - If successful and contains 'v' or '-g': use the result directly
//!      - Example: `v1.0.0` (exact tag)
//!      - Example: `v1.0.0-5-g1a2b3c4d-dirty` (5 commits after v1.0.0 tag)
//!
//! 2. If no tags exist (git describe returns just a commit hash):
//!    - Generate pseudo-version: `v{CARGO_PKG_VERSION}-{timestamp}-{commit}[+dirty]`
//!    - Where:
//!      - `CARGO_PKG_VERSION`: version from Cargo.toml (e.g., "0.1.0")
//!      - `timestamp`: commit timestamp for clean builds, build timestamp for dirty builds
//!      - `commit`: 12-character commit SHA
//!      - `+dirty`: suffix if working directory has uncommitted changes
//!    - Example: `v0.1.0-20250610203045-2adb30a27442+dirty`
//!
//! 3. Smart timestamp selection:
//!    - Clean builds: Use commit timestamp (deterministic, same commit = same version)
//!    - Dirty builds: Use build timestamp (shows when you compiled your changes)
//!
//! 4. Fallback (if git commands fail):
//!    - Generate pseudo-version using current build timestamp
//!
//! This provides meaningful version information for development workflows while
//! respecting semantic versioning when tags are present.

use std::{env, process::Command};

use chrono::Utc;

fn main() {
    ["src", "build.rs", "Cargo.toml", "Cargo.lock"]
        .iter()
        .for_each(|path| println!("cargo:rerun-if-changed={path}"));

    let build_info = generate_human_readable_version();
    println!("cargo:rustc-env=BUILD_INFO_HUMAN={build_info}");
}

/// Executes a git command and returns the trimmed stdout as a String.
fn git_command(args: &[&str]) -> Option<String> {
    Command::new("git")
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Gets the Rust toolchain version
fn get_rustc_version() -> Option<String> {
    Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|s| s.trim().to_string())
}

/// Checks if the working directory has uncommitted changes.
/// Returns None if git is not available or not in a git repository.
///
/// Filters out .cargo-ok which is created by `cargo install --git` in the
/// source checkout directory. This file is not part of the actual source
/// and should not trigger the "dirty" flag during installation, whilst
/// preserving detection of real uncommitted changes during development.
fn is_git_dirty() -> Option<bool> {
    git_command(&["status", "--porcelain"]).map(|output| {
        output.lines().any(|line| {
            let path = &line[3..]; // Skip the status prefix
            // Ignore .cargo-ok file created by cargo install
            path != ".cargo-ok"
        })
    })
}

/// Returns a human-readable Git version string for embedding in build
/// metadata.
///
/// This function attempts to describe the current Git commit using:
/// ```sh
/// git describe --tags --always --dirty
/// ```
///
/// If no tags exist, it generates a pseudo-version using the
/// Cargo.toml version:
/// v{CARGO_PKG_VERSION}-<timestamp>-<commit>+dirty.
fn get_git_version() -> Option<String> {
    git_command(&["describe", "--tags", "--always", "--dirty"])
        .map(|desc| {
            // If git describe returned just a hash (no tags),
            // generate pseudo-version.
            if !desc.contains('v') && !desc.contains("-g") {
                generate_pseudo_version()
            } else {
                desc
            }
        })
        .or_else(|| Some(generate_pseudo_version()))
}

/// Generates a pseudo-version using Cargo.toml version:
/// v{version}-<timestamp>-<commit>+dirty.
fn generate_pseudo_version() -> String {
    let commit_hash =
        git_command(&["rev-parse", "--short=12", "HEAD"]).unwrap_or_else(|| "unknown".to_string());

    let is_dirty = is_git_dirty();

    // Use commit timestamp for clean builds, build timestamp for
    // dirty builds, or build timestamp if git is unavailable.
    let timestamp = match is_dirty {
        Some(true) => {
            // For dirty builds, show when the binary was built (more
            // relevant).
            Utc::now().format("%Y%m%d%H%M%S").to_string()
        }
        Some(false) => {
            // For clean builds, show when the commit was made
            // (deterministic).
            git_command(&["log", "-1", "--format=%ct"])
                .and_then(|s| s.parse::<i64>().ok())
                .and_then(|timestamp| chrono::DateTime::from_timestamp(timestamp, 0))
                .map(|dt| dt.format("%Y%m%d%H%M%S").to_string())
                .unwrap_or_else(|| Utc::now().format("%Y%m%d%H%M%S").to_string())
        }
        None => {
            // No git available, use build timestamp.
            Utc::now().format("%Y%m%d%H%M%S").to_string()
        }
    };

    let dirty_suffix = match is_dirty {
        Some(true) => "+dirty",
        Some(false) => "",
        None => "", // No git available, don't mark as dirty
    };
    let version = env!("CARGO_PKG_VERSION");

    format!("v{version}-{timestamp}-{commit_hash}{dirty_suffix}")
}

/// Generates human-readable version info.
fn generate_human_readable_version() -> String {
    let components = [
        Some(env!("CARGO_PKG_VERSION").to_string()),
        get_git_version().map(|v| format!("({v})")),
        get_rustc_version(),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();

    components.join(" ")
}
