# Building ALTICR

## Quick reference

| Platform | Dev | Production bundle |
|---|---|---|
| macOS | `scripts/dev.sh` | `scripts/build.sh` → `src-tauri/target/release/bundle/macos/` |
| Linux | `scripts/dev.sh` | `scripts/build.sh` → `.../bundle/appimage`, `deb` |
| Windows | `scripts/dev.sh` (PowerShell: `scripts/dev.ps1`) | `cargo tauri build` → NSIS installer |

All scripts live in `scripts/` and are POSIX sh except the Windows helper.

## macOS

```sh
xcode-select --install          # CLT required
npm --prefix frontend install
scripts/build.sh
```

Output: `src-tauri/target/release/bundle/macos/ALTICR.app` (unsigned).
To distribute: sign with `codesign` and notarize, or ship unsigned with
the Gatekeeper instructions in INSTALLATION.md.

## Linux

System dependencies (Debian/Ubuntu):

```sh
sudo apt-get install -y libwebkit2gtk-4.1-dev libappindicator3-dev \
  librsvg2-dev patchelf build-essential curl wget file \
  libssl-dev libgtk-3-dev
```

Then `scripts/build.sh`. Bundles produced: `.deb`, `.AppImage`,
`.rpm` (if tooling present).

On other distros install the equivalent WebKitGTK 4.1 + GTK3 dev packages.

## Windows

1. Install Visual Studio Build Tools (C++ workload) and the WebView2 SDK.
2. `npm --prefix frontend install`
3. `cargo tauri build` (or `scripts/build.ps1`)

Output: `src-tauri/target/release/bundle/nsis/ALTICR_x.y.z_x64-setup.exe`.

## Release profile notes

`scripts/build.sh` builds the frontend (`npm run build`), then
`cargo tauri build` which bundles `frontend/dist` via `frontendDist`.
The dev URL (`localhost:1420`) is only used by `cargo tauri dev`; release
binaries are fully self-contained.
