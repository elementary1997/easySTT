---
name: text-inject
description: Cross-platform text injection for easySTT — inserting transcribed text into the active window. Covers enigo crate, clipboard+paste method vs. direct typing, Windows/Linux specifics, user-configurable injection mode. Use when working on inject.rs or the text injection Tauri command.
version: 1.0.0
---

# Cross-Platform Text Injection

## Overview

Two injection methods, user-selectable in settings:

| Method | Pros | Cons |
|--------|------|------|
| **Clipboard + Paste** | Works everywhere, handles all Unicode | Overwrites clipboard content |
| **Direct typing (enigo)** | Doesn't touch clipboard | Slower for long text, some apps ignore it |

## Cargo.toml

```toml
[dependencies]
enigo = "0.2"
arboard = "3"   # clipboard (more reliable than enigo's clipboard)
```

## inject.rs

```rust
// src-tauri/src/inject.rs
use enigo::{Enigo, Key, Keyboard, Settings};
use arboard::Clipboard;
use std::time::Duration;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq)]
pub enum InjectionMethod {
    ClipboardPaste,
    DirectTyping,
}

pub fn inject_text(text: &str, method: &InjectionMethod) -> anyhow::Result<()> {
    match method {
        InjectionMethod::ClipboardPaste => inject_via_clipboard(text),
        InjectionMethod::DirectTyping => inject_via_typing(text),
    }
}

fn inject_via_clipboard(text: &str) -> anyhow::Result<()> {
    // Save previous clipboard content
    let mut clipboard = Clipboard::new()?;
    let previous = clipboard.get_text().ok();

    // Set new text
    clipboard.set_text(text)?;

    // Small delay to ensure clipboard is ready
    std::thread::sleep(Duration::from_millis(50));

    // Send Ctrl+V
    let mut enigo = Enigo::new(&Settings::default())?;
    enigo.key(Key::Control, enigo::Direction::Press)?;
    enigo.key(Key::Unicode('v'), enigo::Direction::Click)?;
    enigo.key(Key::Control, enigo::Direction::Release)?;

    // Restore previous clipboard after a delay (optional, configurable)
    // std::thread::sleep(Duration::from_millis(200));
    // if let Some(prev) = previous {
    //     clipboard.set_text(prev)?;
    // }

    Ok(())
}

fn inject_via_typing(text: &str) -> anyhow::Result<()> {
    let mut enigo = Enigo::new(&Settings::default())?;
    enigo.text(text)?;
    Ok(())
}
```

## Tauri Command

```rust
// src-tauri/src/main.rs
#[tauri::command]
async fn inject_text(
    text: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let config = state.config.lock().unwrap();
    let method = config.injection_method.clone();
    drop(config);

    // IMPORTANT: release focus from our window first
    // so the target window regains focus before we inject
    std::thread::sleep(std::time::Duration::from_millis(100));

    crate::inject::inject_text(&text, &method)
        .map_err(|e| e.to_string())
}
```

## Window Focus Handling

The floating widget must **lose focus** before injecting, otherwise text goes into our own app.

```rust
// src-tauri/src/main.rs
#[tauri::command]
async fn prepare_inject(app: tauri::AppHandle) {
    // Minimize or hide widget briefly
    if let Some(win) = app.get_webview_window("widget") {
        // On Windows, unfocusing is enough
        // On Linux, we may need to hide
        #[cfg(target_os = "linux")]
        let _ = win.hide();

        #[cfg(target_os = "windows")]
        {
            // Just unfocus — the previously active window will regain focus
            // via the normal Windows focus chain
        }
    }
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
}
```

```typescript
// Frontend: always call prepare_inject before inject_text
const handlePTTRelease = async () => {
  const text = await invoke<string>("stop_recording_and_transcribe");
  await invoke("prepare_inject");
  await invoke("inject_text", { text });
  // Re-show widget after injection
  await getCurrentWindow().show();
};
```

## Linux: Wayland Support

On Wayland, `xdotool` doesn't work. `enigo` uses `atspi` (accessibility API) which works on both X11 and Wayland.

```toml
# Cargo.toml - enigo with atspi feature for Wayland support
enigo = { version = "0.2", features = ["atspi"] }
```

If `atspi` is unavailable, fall back to clipboard method automatically:

```rust
pub fn inject_text_with_fallback(text: &str, preferred: &InjectionMethod) -> anyhow::Result<()> {
    if *preferred == InjectionMethod::DirectTyping {
        match inject_via_typing(text) {
            Ok(()) => return Ok(()),
            Err(_) => {
                // Fallback to clipboard on Wayland/permission issues
                return inject_via_clipboard(text);
            }
        }
    }
    inject_via_clipboard(text)
}
```

## Config Structure

```rust
// src-tauri/src/config.rs
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Config {
    pub injection_method: InjectionMethod,
    pub restore_clipboard: bool,   // restore clipboard after paste
    pub inject_delay_ms: u64,      // delay before injecting (default: 150ms)
    // ... other fields
}

impl Default for Config {
    fn default() -> Self {
        Self {
            injection_method: InjectionMethod::ClipboardPaste,
            restore_clipboard: false,
            inject_delay_ms: 150,
        }
    }
}
```

## Key Rules
- **Always** release window focus before injecting — use a 100-150ms delay
- Default to `ClipboardPaste` — it's more reliable across all apps (terminals, browsers, IDEs)
- On Linux with Wayland: `enigo` with `atspi` feature, or fallback to clipboard
- Don't restore clipboard by default — it causes a second Ctrl+V flash; make it opt-in
- `arboard` is more reliable for clipboard on Windows than enigo's built-in clipboard
- The injection must run on a **separate thread** (not async), as enigo is synchronous
