# Awake

A macOS menu bar app to prevent system sleep. Lightweight, native, no dependencies.

- **Modes**: Prevent display sleep, system sleep, or both
- **Timers**: Auto-deactivate after 15 min, 30 min, 1 hour, or 2 hours
- **Launch at Login**: Optional LaunchAgent integration
- **Interaction**: Left-click to toggle, right-click for menu
- **Tiny**: Single binary, ~1 MB universal (arm64 + x86_64)

Requires **macOS 11+** (Big Sur or later) for SF Symbols support.

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

## Usage

Launch Awake and it appears in your menu bar. Click the icon to:

- Toggle sleep prevention on/off
- Choose a mode (display sleep, system sleep, or both)
- Set a timer for automatic deactivation
- Enable launch at login

## Uninstall

```sh
brew uninstall --cask awake
```

Or delete `Awake.app` from `/Applications` and remove `~/Library/LaunchAgents/io.tmss.awake.plist` if it exists.

## Footprint

Awake is designed to be as lightweight as possible — a single Rust binary with no runtime dependencies, no embedded frameworks, and no background helper processes.

| Metric | Awake | KeepingYouAwake |
|---|---|---|
| App bundle size | **1.2 MB** | 7.6 MB |
| Binary size (universal) | 731 KB | 416 KB |
| Physical memory (active) | 18.8 MB | 26.9 MB (+ `caffeinate`) |
| CPU usage (idle) | 0.0% | 0.0% |
| Processes | 1 | 2 |
| Threads | 4 | 6 |
| Source code | **663 lines** (single file) | ~3,500 lines (Obj-C) |
| Runtime | None (static Rust) | Objective-C runtime |
| Sleep mechanism | IOKit power assertions (direct) | `caffeinate` subprocess |

Awake calls IOKit directly to create power assertions rather than shelling out to `caffeinate`. This means no child processes, no shell overhead, and precise control over assertion types.

Measurements taken on macOS 26.2, Apple Silicon. Physical memory reported by `vmmap --summary`.

## License

MIT
