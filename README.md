# Awake

A macOS menu bar app to prevent system sleep. Lightweight, native, no dependencies.

## Install

### Homebrew (recommended)

```sh
brew tap anatomic/awake
brew install --cask awake
```

### Manual download

Download `Awake.zip` from the [latest release](https://github.com/anatomic/awake/releases/latest), unzip, and move `Awake.app` to `/Applications`.

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

## License

MIT
