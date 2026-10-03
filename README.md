# easySTT

[Русский](README.ru.md) · [Releases](https://github.com/elementary1997/easySTT/releases)

**Push-to-talk** speech-to-text for **Windows** and **Linux** (Tauri 2 + React). Hold a hotkey, speak, get text in the **active window** (clipboard paste or direct typing).

## Features

- **Backends:** local [whisper.cpp](https://github.com/ggerganov/whisper.cpp) (offline), [Cloud.ru](https://cloud.ru) Foundation Models, [OpenRouter](https://openrouter.ai) multimodal audio
- **Global hotkey** for recording without extra windows
- **System tray** settings and quit; no windows shown on startup

## Install

- **Windows:** download the `.exe` / `.msi` from [Releases](https://github.com/elementary1997/easySTT/releases)
- **Linux (Debian/Ubuntu):**
  ```bash
  wget https://github.com/elementary1997/easySTT/releases/latest/download/easystt.deb
  sudo dpkg -i easystt.deb
  ```

## Quick start

1. Start the app — only the tray icon appears
2. Open **Settings** from the tray menu or launch the app again manually
3. Choose a backend and credentials, or a **local** model
4. Default hotkey: `` Alt+` `` — hold while speaking, release to transcribe and inject

**Linux:** global shortcuts and automatic text injection currently use X11. Native Wayland applications are not fully supported; use an X11 session for dictation. The settings UI has been tested on Debian 13 with KDE/Wayland, which does not establish Wayland dictation support.

## Local STT: why it can be slow (and what to do)

| Situation | What to expect |
|-----------|----------------|
| **Release / default build** | **CPU-only** whisper; no GPU in the binary |
| **Model** | Default profile uses **tiny**; **small+** on CPU can take **minutes** per phrase |
| **Speed-up** | Build with **`whisper-vulkan`** (Linux, many GPUs) or **`whisper-cuda`** (NVIDIA) and enable **Use GPU** in settings |
| **Releases** | Some Linux `.deb` builds may be published with Vulkan; generic `.deb` is often **CPU** |

Useful environment variables (see `src-tauri/src/stt/local.rs`):

- `EASYSTT_WHISPER_NO_GPU=1` — force CPU in a GPU-enabled build
- `EASYSTT_WHISPER_FULL_AUDIO_CTX=1` — disable the short-clip fast path (larger `audio_ctx`, closer to default whisper)

Models live under:

- Windows: `%APPDATA%\easystt\models\`
- Linux: `~/.local/share/easystt/models/`

Files: `ggml-tiny.bin`, `ggml-base.bin`, etc. (match the name selected in settings.)

Microphones use the system names from PipeWire/PulseAudio on Linux; speaker monitors and ALSA service aliases are excluded. Select an input in Settings → General, or use the system default. Refresh the list after connecting a device. A small status pill appears at the bottom center during recording and processing, and hides when finished.

## Blank windows on Linux

The app disables WebKitGTK’s DMA-BUF renderer by default to avoid blank windows with affected GPU drivers, including Nouveau on Debian/Wayland. For older installed versions, run `WEBKIT_DISABLE_DMABUF_RENDERER=1 easystt`. Explicit environment overrides are respected; `WEBKIT_DISABLE_DMABUF_RENDERER=0` restores the default renderer. Whisper GPU acceleration is unaffected.

## Build from source

**Needs:** [Rust](https://rustup.rs/) 1.88+, [Node](https://nodejs.org/) 18+

**Linux system packages (Debian/Ubuntu example):**
```bash
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev \
  libasound2-dev libpulse-dev libxdo-dev libssl-dev libclang-dev cmake pkg-config
```

```bash
git clone https://github.com/elementary1997/easySTT.git
cd easySTT
npm install
cargo install tauri-cli@^2 --locked
npm run tauri build
# or: cargo tauri build
```

**GPU (pick one feature when building):**
```bash
npm run tauri:build:gpu     # CUDA / NVIDIA
npm run tauri:build:vulkan  # Vulkan (often Linux)
```

On Linux, whisper.cpp's Vulkan shader generator does not support spaces in the build path. For a checkout under a directory such as `My projects`, use a target path without spaces: `CARGO_TARGET_DIR=/tmp/easystt-target npm run tauri:build:vulkan`.

When an X11 display is available, Linux windows use X11/XWayland by default so the recording indicator can be positioned at the bottom center. An explicit `GDK_BACKEND` setting is respected.

## Development

```bash
npm run tauri dev
# or with CUDA during dev:
npm run tauri:dev:gpu
```

## Tech stack

| Layer | Technology |
|-------|------------|
| UI | React 18, TypeScript, Vite |
| Shell | Tauri 2 |
| Audio | cpal |
| Local STT | whisper-rs (whisper.cpp) |
| Injection | enigo, arboard |
| Config | tauri-plugin-store |

## License

MIT
