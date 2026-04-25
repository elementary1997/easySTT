mod audio;
mod config;
mod inject;
mod stt;

use audio::AudioRecorder;
use config::{model_path, Config, SttBackend};
use stt::{cloudru::CloudRuStt, local::LocalStt, openrouter::OpenRouterStt, SttBackend as Trait};

use std::sync::Mutex;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

pub struct AppState {
    recorder: Mutex<AudioRecorder>,
    config: Mutex<Config>,
}

// ─── Tauri Commands ──────────────────────────────────────────────────────────

#[tauri::command]
async fn start_recording(state: State<'_, AppState>) -> Result<(), String> {
    state
        .recorder
        .lock()
        .unwrap()
        .start()
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn stop_and_transcribe(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let (samples, sample_rate) = state.recorder.lock().unwrap().stop();
    let config = state.config.lock().unwrap().clone();

    if samples.is_empty() {
        return Err("Нет аудиоданных".into());
    }

    let app_clone = app.clone();

    tokio::spawn(async move {
        let result = transcribe_with_config(&samples, sample_rate, &config).await;
        match result {
            Ok(text) if !text.is_empty() => {
                // Inject into active window
                let delay = config.inject_delay_ms;
                let method = config.injection_method.clone();

                // Hide widget briefly so target window regains focus
                #[cfg(target_os = "linux")]
                if let Some(win) = app_clone.get_webview_window("widget") {
                    let _ = win.hide();
                }

                std::thread::sleep(std::time::Duration::from_millis(100));

                let inject_result = inject::inject_text(&text, &method, delay);

                #[cfg(target_os = "linux")]
                if let Some(win) = app_clone.get_webview_window("widget") {
                    let _ = win.show();
                }

                match inject_result {
                    Ok(_) => { let _ = app_clone.emit("transcription-done", &text); }
                    Err(e) => { let _ = app_clone.emit("transcription-error", e.to_string()); }
                }
            }
            Ok(_) => {
                let _ = app_clone.emit("transcription-error", "Речь не распознана");
            }
            Err(e) => {
                let _ = app_clone.emit("transcription-error", e.to_string());
            }
        }
    });

    Ok(())
}

async fn transcribe_with_config(
    samples: &[f32],
    sample_rate: u32,
    config: &Config,
) -> anyhow::Result<String> {
    let lang = config.language.as_str();
    let effective_lang = if lang == "auto" { "" } else { lang };

    match config.stt_backend {
        SttBackend::Cloudru => {
            CloudRuStt {
                api_key: config.cloudru_api_key.clone(),
                base_url: config.cloudru_base_url.clone(),
            }
            .transcribe(samples, sample_rate, effective_lang)
            .await
        }
        SttBackend::Openrouter => {
            OpenRouterStt {
                api_key: config.openrouter_api_key.clone(),
                model: config.openrouter_model.clone(),
            }
            .transcribe(samples, sample_rate, effective_lang)
            .await
        }
        SttBackend::Local => {
            let path = model_path(&config.local_model_name);
            LocalStt {
                model_path: path.to_string_lossy().to_string(),
            }
            .transcribe(samples, sample_rate, effective_lang)
            .await
        }
    }
}

#[tauri::command]
fn apply_config(config: Config, state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    let old_hotkey = state.config.lock().unwrap().hotkey.clone();
    let new_hotkey = config.hotkey.clone();
    *state.config.lock().unwrap() = config;

    if old_hotkey != new_hotkey {
        register_hotkey(&app, &new_hotkey);
    }
    Ok(())
}

#[tauri::command]
fn get_model_path(name: String) -> String {
    config::model_path(&name).to_string_lossy().to_string()
}

#[tauri::command]
fn model_exists(path: String) -> bool {
    std::path::Path::new(&path).exists()
}

#[tauri::command]
fn open_settings(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("settings") {
        win.show().map_err(|e| e.to_string())?;
        win.set_focus().map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ─── Hotkey Registration ─────────────────────────────────────────────────────

fn register_hotkey(app: &AppHandle, hotkey: &str) {
    let gs = app.global_shortcut();

    // Unregister all existing shortcuts first
    let _ = gs.unregister_all();

    let shortcut: tauri_plugin_global_shortcut::Shortcut = match hotkey.parse() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Invalid hotkey '{}': {}", hotkey, e);
            return;
        }
    };

    let app_clone = app.clone();
    let _ = gs.on_shortcut(shortcut, move |_app, _shortcut, event| {
        match event.state() {
            ShortcutState::Pressed => {
                let _ = app_clone.emit("ptt-pressed", ());
            }
            ShortcutState::Released => {
                let _ = app_clone.emit("ptt-released", ());
            }
        }
    });
}

// ─── Tray Setup ──────────────────────────────────────────────────────────────

fn setup_tray(app: &tauri::App) -> anyhow::Result<()> {
    let toggle = MenuItem::with_id(app, "toggle", "Показать / Скрыть", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Настройки", true, None::<&str>)?;
    let separator = tauri::menu::PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Выход", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&toggle, &settings, &separator, &quit])?;

    TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("easySTT")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "toggle" => {
                if let Some(win) = app.get_webview_window("widget") {
                    if win.is_visible().unwrap_or(false) {
                        let _ = win.hide();
                    } else {
                        let _ = win.show();
                        let _ = win.set_focus();
                    }
                }
            }
            "settings" => {
                if let Some(win) = app.get_webview_window("settings") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::DoubleClick { .. } = event {
                let app = tray.app_handle();
                if let Some(win) = app.get_webview_window("widget") {
                    if win.is_visible().unwrap_or(false) {
                        let _ = win.hide();
                    } else {
                        let _ = win.show();
                        let _ = win.set_focus();
                    }
                }
            }
        })
        .build(app)?;

    Ok(())
}

// ─── Entry Point ─────────────────────────────────────────────────────────────

pub fn run() {
    let state = AppState {
        recorder: Mutex::new(AudioRecorder::new()),
        config: Mutex::new(Config::default()),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_shell::init())
        .manage(state)
        .setup(|app| {
            setup_tray(app)?;

            let config = app.state::<AppState>().config.lock().unwrap().clone();
            register_hotkey(app.handle(), &config.hotkey);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start_recording,
            stop_and_transcribe,
            apply_config,
            get_model_path,
            model_exists,
            open_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running easySTT");
}
