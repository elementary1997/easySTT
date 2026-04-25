---
name: tauri-dev
description: Tauri 2 development patterns for easySTT — IPC commands, plugins (store, global-shortcut, tray), permissions, always-on-top windows, Rust state management. Use when writing Rust backend commands, frontend invoke calls, configuring tauri.conf.json, or wiring up plugins.
version: 1.0.0
---

# Tauri 2 Development Patterns for easySTT

## Project Structure

```
easySTT/
├── src/              # React + TypeScript frontend
│   ├── App.tsx
│   ├── components/
│   │   ├── FloatingWidget.tsx
│   │   └── SettingsPanel.tsx
│   └── main.tsx
├── src-tauri/        # Rust backend
│   ├── src/
│   │   ├── main.rs
│   │   ├── audio.rs      # cpal audio capture
│   │   ├── stt/
│   │   │   ├── mod.rs
│   │   │   ├── local.rs  # whisper-rs
│   │   │   ├── cloudru.rs
│   │   │   └── openrouter.rs
│   │   ├── inject.rs     # text injection
│   │   └── config.rs
│   ├── Cargo.toml
│   └── tauri.conf.json
└── package.json
```

## Cargo.toml Dependencies

```toml
[dependencies]
tauri = { version = "2", features = ["tray-icon", "image-png"] }
tauri-plugin-store = "2"
tauri-plugin-global-shortcut = "2"
tauri-plugin-shell = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.12", features = ["multipart", "json"] }
cpal = "0.15"
enigo = "0.2"
anyhow = "1"
```

## IPC: Defining Commands

```rust
// src-tauri/src/main.rs
use tauri::State;
use std::sync::Mutex;

pub struct AppState {
    pub is_recording: Mutex<bool>,
    pub config: Mutex<Config>,
}

#[tauri::command]
async fn start_recording(state: State<'_, AppState>) -> Result<(), String> {
    let mut recording = state.is_recording.lock().unwrap();
    *recording = true;
    // trigger audio capture
    Ok(())
}

#[tauri::command]
async fn stop_recording_and_transcribe(
    state: State<'_, AppState>
) -> Result<String, String> {
    // returns transcribed text
    todo!()
}

#[tauri::command]
fn get_config(state: State<'_, AppState>) -> Config {
    state.config.lock().unwrap().clone()
}

#[tauri::command]
fn save_config(config: Config, state: State<'_, AppState>) -> Result<(), String> {
    *state.config.lock().unwrap() = config;
    Ok(())
}
```

## Registering Commands and Plugins

```rust
fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(AppState {
            is_recording: Mutex::new(false),
            config: Mutex::new(Config::default()),
        })
        .invoke_handler(tauri::generate_handler![
            start_recording,
            stop_recording_and_transcribe,
            get_config,
            save_config,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

## Frontend: Calling Commands

```typescript
import { invoke } from "@tauri-apps/api/core";

// Start recording
await invoke("start_recording");

// Stop and get text
const text = await invoke<string>("stop_recording_and_transcribe");

// Get config
const config = await invoke<AppConfig>("get_config");

// Save config
await invoke("save_config", { config: newConfig });
```

## System Tray

```rust
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, TrayIconBuilder, TrayIconEvent},
    Manager,
};

fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let toggle = MenuItem::with_id(app, "toggle", "Toggle Widget", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&toggle, &settings, &quit])?;

    TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "toggle" => { /* toggle floating window */ }
            "settings" => { /* open settings window */ }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}
```

## Always-on-Top Floating Window

```rust
// In tauri.conf.json
{
  "windows": [
    {
      "label": "widget",
      "title": "easySTT",
      "width": 200,
      "height": 80,
      "alwaysOnTop": true,
      "decorations": false,
      "transparent": true,
      "resizable": false,
      "skipTaskbar": true
    },
    {
      "label": "settings",
      "title": "easySTT Settings",
      "width": 500,
      "height": 600,
      "visible": false
    }
  ]
}
```

```rust
// Toggle widget visibility from Rust
#[tauri::command]
fn toggle_widget(app: tauri::AppHandle) {
    let window = app.get_webview_window("widget").unwrap();
    if window.is_visible().unwrap() {
        window.hide().unwrap();
    } else {
        window.show().unwrap();
        window.set_focus().unwrap();
    }
}
```

## Global Shortcut (PTT)

```rust
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

fn register_ptt(app: &tauri::App, hotkey: &str) -> Result<(), anyhow::Error> {
    let shortcut: Shortcut = hotkey.parse()?;
    app.global_shortcut().on_shortcut(shortcut, |app, _shortcut, event| {
        match event.state() {
            ShortcutState::Pressed => {
                let _ = app.emit("ptt-pressed", ());
            }
            ShortcutState::Released => {
                let _ = app.emit("ptt-released", ());
            }
        }
    })?;
    Ok(())
}
```

```typescript
// Frontend listening for PTT events
import { listen } from "@tauri-apps/api/event";

await listen("ptt-pressed", () => invoke("start_recording"));
await listen("ptt-released", () => invoke("stop_recording_and_transcribe")
  .then(text => invoke("inject_text", { text })));
```

## Config Persistence with tauri-plugin-store

```typescript
import { Store } from "@tauri-apps/plugin-store";

const store = await Store.load("settings.json");

// Save
await store.set("stt_backend", "cloudru");
await store.set("cloudru_api_key", "...");
await store.save();

// Load
const backend = await store.get<string>("stt_backend") ?? "local";
```

## tauri.conf.json Permissions

```json
{
  "bundle": {
    "identifier": "com.easystt.app"
  },
  "app": {
    "security": {
      "csp": null
    }
  },
  "plugins": {
    "store": {},
    "global-shortcut": {
      "shortcuts": []
    }
  }
}
```

## Emitting Events from Rust to Frontend

```rust
// Emit recording status updates
app_handle.emit("recording-status", serde_json::json!({
    "status": "recording",
    "duration_ms": elapsed
})).unwrap();

// Emit transcription result
app_handle.emit("transcription-done", serde_json::json!({
    "text": transcribed_text,
    "backend": "cloudru"
})).unwrap();
```

```typescript
import { listen } from "@tauri-apps/api/event";

await listen<{status: string, duration_ms: number}>("recording-status", (e) => {
  setRecordingStatus(e.payload.status);
});

await listen<{text: string}>("transcription-done", (e) => {
  setLastTranscription(e.payload.text);
});
```

## Key Rules
- Always use `async` Tauri commands when doing I/O (audio, HTTP)
- Use `Mutex<T>` for shared state; prefer `tokio::sync::Mutex` for async commands
- Emit events for long-running operations (recording progress) rather than blocking invoke
- Window label `"widget"` = floating button, `"settings"` = settings panel
- Store sensitive data (API keys) via tauri-plugin-store, not hardcoded
