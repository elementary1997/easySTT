# easySTT — Claude Development Guide

## What is this

Cross-platform Speech-to-Text utility. User holds a hotkey or clicks a button → speaks → transcribed text is injected into the currently active window.

**Targets:** Windows (.exe/.msi) + Linux (.deb). Binary < 5 MB, models downloaded separately.

## Tech Stack

| Layer | Technology |
|-------|-----------|
| UI | React 18 + TypeScript + Vite |
| Desktop shell | Tauri 2 (Rust) |
| Audio capture | `cpal` crate |
| Local STT | `whisper-rs` (whisper.cpp); optional GPU: `--features whisper-cuda` / `whisper-vulkan` / `whisper-metal` |
| Cloud STT | Cloud.ru Foundation Models (Whisper-large-v3) |
| Alt cloud STT | OpenRouter (multimodal audio proxy) |
| Text injection | `enigo` + `arboard` |
| Config storage | `tauri-plugin-store` |
| Global hotkey | `tauri-plugin-global-shortcut` |

## Project Structure

```
easySTT/
├── src/                     # React frontend
│   ├── components/
│   │   ├── FloatingWidget.tsx   # always-on-top PTT button
│   │   └── SettingsPanel.tsx    # full settings UI
│   ├── hooks/
│   │   └── useRecording.ts
│   ├── lib/
│   │   └── store.ts             # tauri-plugin-store wrapper
│   ├── App.tsx
│   └── main.tsx
├── src-tauri/
│   ├── src/
│   │   ├── main.rs
│   │   ├── audio.rs             # cpal recording
│   │   ├── inject.rs            # text injection (enigo + arboard)
│   │   ├── config.rs            # Config struct + persistence
│   │   └── stt/
│   │       ├── mod.rs           # STTBackend trait + Backend enum
│   │       ├── local.rs         # whisper-rs
│   │       ├── cloudru.rs       # Cloud.ru REST API
│   │       └── openrouter.rs    # OpenRouter REST API
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   └── capabilities/
│       └── default.json
├── .claude/
│   └── skills/                  # project-scoped Claude skills
│       ├── tauri-dev/
│       ├── audio-stt/
│       ├── text-inject/
│       ├── cloud-stt-apis/
│       └── tauri-packaging/
├── .github/workflows/
│   ├── build.yml                # CI: build on every push
│   └── release.yml              # Release: tag → exe + deb artifacts
├── README.md                    # English
├── README.ru.md                 # Russian
├── CLAUDE.md                    # this file
└── package.json
```

## Key Architecture Decisions

### Two windows
- `widget` — 220×90px, always-on-top, no decorations, transparent, skipTaskbar
- `settings` — 520×640px, normal window, hidden by default

### STT backends (user picks one in settings)
1. **Local** — whisper-rs. Models stored in OS data dir, never bundled. Supports `tiny/base/small/medium/large-v3`.
2. **Cloud.ru** — POST to their Whisper-large-v3 endpoint (OpenAI-compatible format). Best Russian quality.
3. **OpenRouter** — multipart audio OR base64 chat depending on model. User provides model ID.

### Text injection (user picks one in settings)
- **ClipboardPaste** (default) — copies text, sends Ctrl+V. Works everywhere. Doesn't restore clipboard.
- **DirectTyping** — enigo types characters. Cleaner but slower for long text.

### PTT flow
```
hotkey/button pressed → start_recording (cpal)
hotkey/button released → stop + get samples → transcribe → prepare_inject → inject_text
```

## Build Commands

```bash
# Install deps (first time)
npm install
cargo install tauri-cli --version "^2.0"

# Dev mode (hot reload)
cargo tauri dev

# Build release (current platform)
cargo tauri build

# Local Whisper on GPU (one of: whisper-cuda, whisper-vulkan, whisper-metal)
cargo tauri build -- --features whisper-cuda

# Output:
#   Linux:   src-tauri/target/release/bundle/deb/*.deb
#   Windows: src-tauri/target/release/bundle/nsis/*.exe
```

## CI / Releases

- **Build CI**: `.github/workflows/build.yml` — runs on every push to `main`
- **Release**: `.github/workflows/release.yml` — push a tag `v*` → creates GitHub Release with `.exe` and `.deb` artifacts

## Tauri Permissions

All capabilities are in `src-tauri/capabilities/default.json`. When adding new plugins, add their permissions there — Tauri 2 requires explicit capability grants.

## Rust Patterns

- All Tauri commands that do I/O must be `async`
- Shared state: `Mutex<T>` in `tauri::State`. For async commands use `tokio::sync::Mutex`
- Emit events for long-running ops (recording progress) — don't block invoke
- `AudioRecorder` holds a `cpal::Stream` which is `!Send` — keep it in `std::sync::Mutex` on a dedicated thread

## Cross-Platform Notes

| Concern | Windows | Linux |
|---------|---------|-------|
| Audio backend | WASAPI (via cpal) | PulseAudio/ALSA (via cpal) |
| Text injection | WinAPI via enigo | X11/Wayland via enigo+atspi |
| Tray icon | native | libayatana-appindicator3 |
| Config dir | `%APPDATA%\easySTT` | `~/.config/easystt` |
| Models dir | `%APPDATA%\easySTT\models` | `~/.local/share/easystt/models` |

## Languages

- App UI: Russian + English (i18n-ready, but start with Russian)
- STT language: configurable per-session (`ru` default, `en` optional)
- Whisper language codes: `ru`, `en`, `""` (auto-detect)

## Skills Available (project-scoped)

Use these when working in the corresponding area:
- `/tauri-dev` — IPC, plugins, tray, shortcuts, windows
- `/audio-stt` — cpal, whisper-rs, PCM→WAV
- `/text-inject` — enigo, clipboard, Wayland
- `/cloud-stt-apis` — Cloud.ru + OpenRouter API
- `/tauri-packaging` — GitHub Actions, exe/deb
