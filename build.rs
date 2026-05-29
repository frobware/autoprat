//! Build script for autoprat - generates version information.
//!
//! ## Version string
//!
//! The package version from `Cargo.toml` (`CARGO_PKG_VERSION`) is the
//! source of truth and always leads. Git provenance, when available, is
//! appended in parentheses, and a `built <timestamp>` marker is added
//! for development builds -- any debug-profile build, or any build with
//! uncommitted changes. Everything degrades cleanly when git or tags are
//! absent:
//!
//! - release, exact tag:    `0.2.1 (v0.2.1)`
//! - release, ahead/no tag: `0.2.1 (v0.2.1-3-gabc123)` / `0.2.1 (gabc123)`
//! - debug build (clean):   `0.2.1 (gabc123 built 20260529T134500Z)`
//! - dirty build (any):     `0.2.1 (gabc123+dirty built 20260529T134500Z)`
//! - no git at all:         `0.2.1` (release) / `0.2.1 (built 20260529T134500Z)` (debug)
//!
//! The rustc version is appended when available. clap prefixes the
//! whole string with the binary name, so `--version` reads e.g.
//! `autoprat 0.2.1 (v0.2.1) rustc 1.95.0 (...)`.
//!
//! Leading with `CARGO_PKG_VERSION` rather than `git describe` keeps the
//! version correct in every install path: crates.io tarballs have no
//! `.git`, and `cargo install --git` checkouts have no tag ref, so a
//! describe-first scheme would silently drop to a hash there. The git
//! suffix distinguishes a tagged build from one ahead of, or dirty
//! against, a tag; the timestamp marks when a development binary was
//! compiled. A debug build always counts as development and is stamped,
//! even on an exact clean tag -- the dev profile wins over
//! tag-exactness. Only a clean release build -- including `cargo
//! install` -- is reproducible and timestamp-free.

use std::{env, path::Path, process::Command};

use chrono::Utc;

fn main() {
    ["src", "build.rs", "Cargo.toml", "Cargo.lock"]
        .iter()
        .for_each(|path| println!("cargo:rerun-if-changed={path}"));

    watch_git_state();

    let build_info = generate_human_readable_version();
    println!("cargo:rustc-env=BUILD_INFO_HUMAN={build_info}");
}

/// Tells Cargo to rerun this script when git state moves, so the
/// embedded version does not go stale.
///
/// The version string captures git describe, the short hash and the
/// dirty flag, none of which Cargo's fingerprint of the source files
/// above tracks. Without this, building dirty and then committing
/// (without touching a watched file) would leave a stale hash, describe
/// and `+dirty` in the binary. Watching the refs that move HEAD --
/// commits, checkouts, resets via `logs/HEAD`; branch switches via
/// `HEAD`; staging via `index` -- forces a refresh.
///
/// Paths are resolved with `git rev-parse --git-path` so this is
/// correct in worktrees and submodules, where `.git` is a file pointing
/// elsewhere, and only emitted when they exist: Cargo treats a missing
/// `rerun-if-changed` path as always-changed, which would rebuild every
/// time (and there is no git at all in a crates.io tarball).
///
/// This does not catch a working tree dirtied by editing a file outside
/// the watched source set -- staging updates `index`, but a bare edit to
/// e.g. README will not refresh the dirty flag until a watched file
/// changes. Catching that would mean rerunning unconditionally, which
/// defeats build caching.
fn watch_git_state() {
    for spec in ["HEAD", "logs/HEAD", "index"] {
        if let Some(path) = git_command(&["rev-parse", "--git-path", spec])
            && Path::new(&path).exists()
        {
            println!("cargo:rerun-if-changed={path}");
        }
    }
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

/// Gets the Rust toolchain version.
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
/// Filters out .cargo-ok which is created by `cargo install --git` in
/// the source checkout directory. This file is not part of the actual
/// source and should not trigger the "dirty" flag during installation,
/// whilst preserving detection of real uncommitted changes during
/// development.
fn is_git_dirty() -> Option<bool> {
    git_command(&["status", "--porcelain"]).map(|output| {
        output.lines().any(|line| {
            let path = &line[3..]; // Skip the status prefix.
            path != ".cargo-ok"
        })
    })
}

/// Returns the git provenance, or None when no git information is
/// available at all.
///
/// Prefers `git describe --tags`, which yields `v0.2.1` on a tagged
/// commit or `v0.2.1-3-gabc123` ahead of one. When no tag is reachable
/// -- as in a `cargo install --git` checkout, whose tag refs are absent
/// -- it falls back to the short commit hash as `gabc123`. A `+dirty`
/// suffix marks uncommitted changes.
fn git_provenance(dirty: bool) -> Option<String> {
    let mut provenance = git_command(&["describe", "--tags"]).or_else(|| {
        git_command(&["rev-parse", "--short", "HEAD"]).map(|hash| format!("g{hash}"))
    })?;

    if dirty {
        provenance.push_str("+dirty");
    }

    Some(provenance)
}

/// Returns a `built <timestamp>` marker for development builds.
///
/// A build counts as development if it is debug-profile (`cargo
/// build`/`cargo test`, where `PROFILE` is `debug`) or has uncommitted
/// changes. Clean release builds -- `cargo build --release`, `cargo
/// install` -- return None and stay timestamp-free, so installed
/// binaries are reproducible while local dev binaries show when they
/// were compiled.
fn build_timestamp(dirty: bool) -> Option<String> {
    let is_debug = matches!(env::var("PROFILE").as_deref(), Ok("debug"));
    (is_debug || dirty).then(|| format!("built {}", Utc::now().format("%Y%m%dT%H%M%SZ")))
}

/// Combines git provenance and the development build timestamp into the
/// parenthetical metadata, or None when there is nothing to show.
fn version_metadata() -> Option<String> {
    let dirty = is_git_dirty() == Some(true);

    let parts: Vec<String> = [git_provenance(dirty), build_timestamp(dirty)]
        .into_iter()
        .flatten()
        .collect();

    (!parts.is_empty()).then(|| parts.join(" "))
}

/// Generates the human-readable version string embedded as
/// BUILD_INFO_HUMAN and surfaced by `--version`.
fn generate_human_readable_version() -> String {
    [
        Some(env!("CARGO_PKG_VERSION").to_string()),
        version_metadata().map(|m| format!("({m})")),
        get_rustc_version(),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" ")
}
