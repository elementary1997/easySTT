---
name: tauri-packaging
description: Building and packaging easySTT for Windows (.exe/.msi) and Linux (.deb) using Tauri 2 and GitHub Actions. Covers tauri.conf.json bundle config, version bumping, CI matrix, code signing stubs. Use when setting up builds, configuring tauri.conf.json bundle section, or writing GitHub Actions workflows.
version: 1.0.0
---

# Tauri Packaging for Windows and Linux

## tauri.conf.json — Bundle Configuration

```json
{
  "productName": "easySTT",
  "version": "0.1.0",
  "identifier": "com.easystt.app",
  "build": {
    "frontendDist": "../dist",
    "devUrl": "http://localhost:1420",
    "beforeDevCommand": "npm run dev",
    "beforeBuildCommand": "npm run build"
  },
  "bundle": {
    "active": true,
    "targets": "all",
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ],
    "resources": [],
    "externalBin": [],
    "copyright": "",
    "category": "Utility",
    "shortDescription": "Speech to text for any window",
    "longDescription": "Convert speech to text with a hotkey and insert into any active window",
    "windows": {
      "certificateThumbprint": null,
      "digestAlgorithm": "sha256",
      "timestampUrl": "",
      "wix": {},
      "nsis": {
        "installerIcon": "icons/icon.ico",
        "headerImage": null,
        "sidebarImage": null,
        "installMode": "currentUser",
        "languages": ["English", "Russian"],
        "displayLanguageSelector": false
      }
    },
    "linux": {
      "deb": {
        "depends": ["libwebkit2gtk-4.1-0", "libgtk-3-0", "libayatana-appindicator3-1"],
        "section": "utils",
        "priority": "optional",
        "desktopTemplate": null
      },
      "appimage": {
        "bundleMediaFramework": false
      }
    }
  }
}
```

## Local Build Commands

```bash
# Install Tauri CLI
cargo install tauri-cli --version "^2.0"

# Development
cargo tauri dev

# Build for current platform
cargo tauri build

# Build specific target
cargo tauri build --target x86_64-pc-windows-msvc   # Windows
cargo tauri build --target x86_64-unknown-linux-gnu  # Linux deb

# Output locations:
# Windows: src-tauri/target/release/bundle/nsis/*.exe
#          src-tauri/target/release/bundle/msi/*.msi
# Linux:   src-tauri/target/release/bundle/deb/*.deb
```

## GitHub Actions — Cross-Platform Build

```yaml
# .github/workflows/release.yml
name: Release

on:
  push:
    tags:
      - 'v*'

jobs:
  build:
    strategy:
      fail-fast: false
      matrix:
        include:
          - platform: ubuntu-22.04
            target: x86_64-unknown-linux-gnu
            artifact: "*.deb"
          - platform: windows-latest
            target: x86_64-pc-windows-msvc
            artifact: "*.exe"

    runs-on: ${{ matrix.platform }}

    steps:
      - uses: actions/checkout@v4

      - name: Install Linux dependencies
        if: matrix.platform == 'ubuntu-22.04'
        run: |
          sudo apt-get update
          sudo apt-get install -y \
            libwebkit2gtk-4.1-dev \
            libgtk-3-dev \
            libayatana-appindicator3-dev \
            librsvg2-dev \
            pkg-config

      - name: Setup Node.js
        uses: actions/setup-node@v4
        with:
          node-version: '20'
          cache: 'npm'

      - name: Setup Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}

      - name: Rust cache
        uses: swatinem/rust-cache@v2
        with:
          workspaces: './src-tauri -> target'

      - name: Install frontend dependencies
        run: npm ci

      - name: Build Tauri app
        uses: tauri-apps/tauri-action@v0
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        with:
          tagName: ${{ github.ref_name }}
          releaseName: 'easySTT ${{ github.ref_name }}'
          releaseBody: 'See changelog for details.'
          releaseDraft: true
          prerelease: false
          args: --target ${{ matrix.target }}
```

## Dev Build Workflow (no release, just artifacts)

```yaml
# .github/workflows/build.yml
name: Build

on:
  push:
    branches: [main]
  pull_request:

jobs:
  build:
    strategy:
      matrix:
        include:
          - platform: ubuntu-22.04
          - platform: windows-latest
    runs-on: ${{ matrix.platform }}
    steps:
      - uses: actions/checkout@v4

      - name: Install Linux deps
        if: matrix.platform == 'ubuntu-22.04'
        run: |
          sudo apt-get update && sudo apt-get install -y \
            libwebkit2gtk-4.1-dev libgtk-3-dev \
            libayatana-appindicator3-dev librsvg2-dev pkg-config

      - uses: actions/setup-node@v4
        with:
          node-version: '20'
          cache: 'npm'

      - uses: dtolnay/rust-toolchain@stable

      - uses: swatinem/rust-cache@v2
        with:
          workspaces: './src-tauri -> target'

      - run: npm ci

      - run: npm run build  # frontend only check

      - name: Build Tauri (no bundle for speed)
        run: cargo build --release
        working-directory: src-tauri
```

## Version Bumping

```bash
# Update version in both places:
# 1. src-tauri/tauri.conf.json → "version"
# 2. src-tauri/Cargo.toml → [package] version

# Then tag:
git tag v0.2.0
git push origin v0.2.0
# → triggers release workflow
```

## Linux .desktop File (auto-generated by Tauri)

Tauri generates `/usr/share/applications/easystt.desktop` in the .deb.
For system tray to work on Ubuntu: ensure `libayatana-appindicator3-1` is in deb depends.

## Key Rules
- Linux build **must** run on `ubuntu-22.04` (not 24.04) — webkit2gtk-4.1 availability
- Always include `libayatana-appindicator3-dev` in Linux deps for tray icon support
- Use `tauri-apps/tauri-action@v0` for releases — it handles draft creation automatically
- whisper.cpp models are NOT bundled — document download location for users
- App binary is < 5 MB; models are separate downloads (75 MB – 3 GB)
- Test `--target x86_64-unknown-linux-gnu` even on Linux CI (explicit is safer than default)
