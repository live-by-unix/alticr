# Releasing ALTICR

## One-time setup

1. Tag-driven: push a tag `vX.Y.Z` matching `version` in
   `src-tauri/tauri.conf.json` and `frontend/package.json`.
2. For signed macOS/Windows artifacts, add signing secrets to the repo
   (see Tauri docs); unsigned builds still produce usable artifacts.

## Local release build

```sh
scripts/build.sh
```

Artifacts land in `src-tauri/target/release/bundle/`:

- macOS: `ALTICR.app` + `.dmg`
- Linux: `.deb`, `.AppImage`, `.rpm`
- Windows: NSIS `-setup.exe`

## GitHub Actions release workflow

`.github/workflows/release.yml` (add it when cutting the first release):

```yaml
name: Release
on:
  push:
    tags: ['v*']
jobs:
  build:
    strategy:
      matrix:
        include:
          - os: macos-latest
          - os: ubuntu-latest
          - os: windows-latest
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: actions/setup-node@v4
        with: { node-version: 22 }
      - run: npm --prefix frontend ci && npm --prefix frontend run build
      - run: cargo tauri build
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
      - uses: softprops/action-gh-release@v2
        with:
          files: |
            src-tauri/target/release/bundle/**/*.dmg
            src-tauri/target/release/bundle/**/*.deb
            src-tauri/target/release/bundle/**/*.AppImage
            src-tauri/target/release/bundle/**/*.exe
            src-tauri/target/release/bundle/macos/*.app.tar.gz
```

## Checklist before tagging

- [ ] `CHANGELOG.md` updated
- [ ] Version bumped in `tauri.conf.json`, `Cargo.toml`, `package.json`
- [ ] `scripts/lint.sh` and `scripts/test.sh` green
- [ ] Production build produces bundles on all target OSes
