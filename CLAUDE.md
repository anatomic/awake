# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Awake is a macOS menu bar app that prevents system sleep. Written in Rust using native macOS frameworks (IOKit for power assertions, AppKit for UI). Single-file implementation (~600 lines in `src/main.rs`). Targets macOS 11+, ships as a universal binary (arm64 + x86_64).

## Setup

After cloning, run `make setup` to configure git hooks (runs `cargo fmt --check` and `cargo clippy` before each commit).

## Build Commands

```bash
cargo build --release          # Native build
cargo fmt --check              # Check formatting
cargo clippy --all-targets -- -D warnings  # Lint (strict)
cargo test                     # Run tests
make universal                 # Universal binary (arm64 + x86_64)
make bundle                    # App bundle with universal binary
make package                   # Bundle + ZIP
make clean                     # Clean all artifacts
```

## Architecture

All code lives in `src/main.rs` with three core components:

1. **Sleep Prevention** — IOKit power assertions with three modes (Display, System, Display+System). Uses `AtomicU32` for assertion ID tracking.

2. **Menu Bar UI** — AppKit `NSStatusBar` status item. Left-click toggles, right-click opens menu. Custom `AwakeDelegate` Objective-C class registered at runtime handles actions. Icons: moon (sleep) / coffee (awake, SF Symbol).

3. **Timer System** — Background thread with 1-second sleep intervals for responsive cancellation. `AtomicU64` expiry time. Supports 15m/30m/1h/2h durations.

Additional: Launch-at-login via LaunchAgent plist at `~/Library/LaunchAgents/io.tmss.awake.plist`.

**Key patterns**: Global atomic state (no locks), unsafe `RawId` wrapper for ObjC object storage with Send+Sync, `MainThreadMarker` for AppKit thread safety.

## Release Process

Update version in `Cargo.toml`, commit, tag `vX.Y.Z`, push. GitHub Actions builds, signs, notarizes, creates release, and updates Homebrew cask automatically.

## Issue Tracking

Uses **bd** (beads) for git-backed issue tracking. See AGENTS.md for workflow. Key commands: `bd ready`, `bd show <id>`, `bd close <id>`, `bd sync`.
