# Installation

## Prebuilt binaries

Download the latest release for your platform from the
[GitHub Releases](https://github.com/live-by-unix/alticr/releases) page:

- **macOS**: `ALTICR_x.y.z_aarch64.dmg` (Apple Silicon) or `.app` tarball
- **Linux**: AppImage or `.deb`
- **Windows**: NSIS installer or portable `.exe`

## macOS Gatekeeper note

ALTICR is not yet notarized. On first launch, right-click the app and
choose **Open**, or run:

```sh
xattr -dr com.apple.quarantine /Applications/ALTICR.app
```

## Build from source

See [BUILDING.md](BUILDING.md) for platform-specific instructions.

## Requirements

- A POSIX shell (`zsh`, `bash`, `fish`, …) or a configured custom shell.
- Rust 1.85+ and Node.js 20+ to build from source.
- On Linux: WebKitGTK 4.1 development packages (see BUILDING.md).
