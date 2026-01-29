# Awake

A macOS menu bar app that prevents system sleep. Written in Rust, single binary, no dependencies.

Left-click the menu bar icon to toggle sleep prevention on or off. Right-click for options - choose between preventing display sleep, system sleep, or both, set a timer (15 min, 30 min, 1 or 2 hours), or enable launch at login.

The whole thing is about 660 lines of Rust in a single file, shipping as a ~1 MB universal binary (arm64 + x86_64). Requires macOS 11+ for SF Symbols support.

## Install

```sh
brew install anatomic/awake/awake
```

This installs Awake as a Homebrew cask from the [anatomic/awake tap](https://github.com/anatomic/homebrew-awake). To update later, run `brew upgrade awake`.

Alternatively, download `Awake.zip` from the [latest release](https://github.com/anatomic/awake/releases/latest), unzip, and move `Awake.app` to `/Applications`.

## Build from source

Requires Rust and macOS.

```sh
# Native build (current architecture only)
cargo build --release

# Universal binary (arm64 + x86_64)
make universal

# Full app bundle
make bundle

# App bundle + ZIP
make package
```

## Uninstall

```sh
brew uninstall --cask awake
```

Or delete `Awake.app` from `/Applications` and remove `~/Library/LaunchAgents/io.tmss.awake.plist` if it exists.

## Footprint

Awake calls IOKit directly to create power assertions rather than shelling out to `caffeinate`. This means no child processes, no shell overhead, and precise control over assertion types.

Here's how it compares to [KeepingYouAwake](https://keepingyouawake.app) when both are actively preventing sleep:

| Metric | Awake | KeepingYouAwake |
|---|---|---|
| App bundle size | 1.2 MB | 7.6 MB |
| Binary size (universal) | 731 KB | 416 KB |
| Physical memory (active) | 18.8 MB | 26.9 MB (+ `caffeinate`) |
| CPU usage (idle) | 0.0% | 0.0% |
| Processes | 1 | 2 |
| Threads | 4 | 6 |
| Source code | 663 lines (single file) | ~3,500 lines (Obj-C) |
| Sleep mechanism | IOKit power assertions (direct) | `caffeinate` subprocess |

KeepingYouAwake is a well-maintained project with a significantly more mature codebase. If you need something reliable and battle-tested, use that. These numbers are shared out of curiosity, not competition - Awake is just a small side project exploring what a minimal Rust implementation looks like.

Measurements taken on macOS 26.2, Apple Silicon. Physical memory reported by `vmmap --summary`.

## Licence

MIT
