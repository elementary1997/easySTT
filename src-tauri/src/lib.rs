mod agent_api;
mod audio;
mod config;
mod inject;
mod plugin_manager;
mod stt;
#[cfg(windows)]
mod win_widget;

use audio::AudioRecorder;
use config::{model_path, models_dir, Config, PluginEntry, SttBackend};
use serde::Serialize;
use stt::cloudru::{bearer_for_stt, fetch_model_ids, CloudRuStt, normalize_fm_base_url};
use stt::local::{transcribe_cached, LocalWhisperCache};
use stt::model_catalog::{model_catalog, ModelInfo};
use stt::openrouter::OpenRouterStt;
use stt::translate::{translate_via_cloudru, translate_via_openrouter};
use stt::SttBackend as Trait;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::utils::config::Color;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State,
};
use serde_json::json;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tauri_plugin_store::StoreExt;

/// #1a1a2e — дефолтный фон виджета (см. tauri.conf.json backgroundColor).
#[allow(dead_code)]
const WIDGET_BG: Color = Color(26, 26, 46, 255);

/// Разбирает "#RRGGBB" → Color. Возвращает None для невалидных строк.
#[allow(dead_code)]
fn parse_hex_color(hex: &str) -> Option<Color> {
    let h = hex.trim().trim_start_matches('#');
    if h.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&h[0..2], 16).ok()?;
    let g = u8::from_str_radix(&h[2..4], 16).ok()?;
    let b = u8::from_str_radix(&h[4..6], 16).ok()?;
    Some(Color(r, g, b, 255))
}

pub struct AppState {
    recorder: Mutex<AudioRecorder>,
    config: Mutex<Config>,
    /// Reused [WhisperContext] for the active local model (see [transcribe_cached]).
    local_whisper: Arc<Mutex<Option<LocalWhisperCache>>>,
    /// Set from UI to cancel the in-flight transcription (local: whisper abort; cloud: drop HTTP).
    pub transcription_cancel: Arc<AtomicBool>,
    /// Set to true to cancel an in-progress model download.
    pub download_cancel: Arc<AtomicBool>,
    /// HWND (as isize) of the foreground window at PTT press time.
    prev_foreground_hwnd: Mutex<isize>,
    /// True when the app was launched by the OS autostart mechanism (--autostart arg).
    pub is_autostart: bool,
    /// Менеджер дочерних процессов плагинов.
    pub plugin_manager: plugin_manager::PluginManager,
    /// Второй рекордер для фонового прослушивания голосовых команд плагинов.
    always_on_recorder: Mutex<AudioRecorder>,
    /// True когда фоновая задача always-on запущена.
    always_on_active: Arc<AtomicBool>,
}

fn is_cancelled_msg(s: &str) -> bool {
    let s = s.to_lowercase();
    s.contains("отмен") || s.contains("cancell")
}

/// Скрываем индикатор записи через `delay_ms` после завершения транскрипции.
async fn auto_hide_indicator(app: &AppHandle, delay_ms: u64) {
    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
    if let Some(w) = app.get_webview_window("indicator") {
        let _ = w.hide();
    }
}

/// Completes when [AppState::transcription_cancel] becomes true.
async fn until_user_cancel(cancel: Arc<AtomicBool>) {
    while !cancel.load(Ordering::SeqCst) {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

// ─── Tauri Commands ──────────────────────────────────────────────────────────

#[tauri::command]
async fn start_recording(state: State<'_, AppState>) -> Result<(), String> {
    // Capture the currently focused window BEFORE recording starts.
    // The user is actively typing in some editor — that's our injection target.
    // We'll restore focus there after transcription instead of hiding the widget.
    #[cfg(windows)]
    {
        use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
        let hwnd = unsafe { GetForegroundWindow() };
        *state.prev_foreground_hwnd.lock().unwrap() = hwnd.0 as isize;
    }

    let device_name = state.config.lock().unwrap().mic_device_name.clone();
    state
        .recorder
        .lock()
        .unwrap()
        .start(&device_name)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn list_microphones() -> Vec<String> {
    audio::list_microphones()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LocalSttBuildInfo {
    /// `true` if built with one of `whisper-cuda` / `whisper-vulkan` / `whisper-metal`.
    build_has_gpu: bool,
    /// `"cuda"` / `"vulkan"` / `"metal"`, or `null` in CPU-only builds.
    build_gpu_kind: Option<&'static str>,
    /// `EASYSTT_WHISPER_NO_GPU=1` forces CPU in GPU-enabled binaries.
    env_forces_cpu: bool,
}

#[tauri::command]
fn get_local_stt_build_info() -> LocalSttBuildInfo {
    let (build_has_gpu, build_gpu_kind) = if cfg!(feature = "whisper-cuda") {
        (true, Some("cuda"))
    } else if cfg!(feature = "whisper-vulkan") {
        (true, Some("vulkan"))
    } else if cfg!(feature = "whisper-metal") {
        (true, Some("metal"))
    } else {
        (false, None)
    };
    let env_forces_cpu = std::env::var("EASYSTT_WHISPER_NO_GPU").as_deref() == Ok("1");
    LocalSttBuildInfo {
        build_has_gpu,
        build_gpu_kind,
        env_forces_cpu,
    }
}

#[tauri::command]
fn get_local_accel_info() -> String {
    let build_ru = if cfg!(feature = "whisper-cuda") {
        "в бинаре собрана поддержка CUDA (NVIDIA)"
    } else if cfg!(feature = "whisper-vulkan") {
        "в бинаре собрана поддержка Vulkan (часто AMD/NVIDIA/Intel на Linux; проверяйте драйверы)"
    } else if cfg!(feature = "whisper-metal") {
        "в бинаре собрана Metal (только macOS с GPU Apple)"
    } else {
        "в этом бинаре нет GPU-ускорителя: только CPU (релизы/типичная `cargo tauri build` — так и есть)"
    };
    format!("{build_ru}. \
Без ускорителя модели small/medium/large на CPU идут очень долго: используйте tiny/base или \
пересоберите: `npm run tauri:build:vulkan` / `whisper-cuda` и отметьте «GPU» в настройках. \
EASYSTT_WHISPER_NO_GPU=1 — принудительно CPU даже в GPU-сборке.")
}

#[derive(Serialize, Clone)]
struct TestCloudruResult {
    message: String,
    models: Vec<String>,
}

#[tauri::command]
async fn test_cloudru(
    api_key: String,
    key_id: String,
    base_url: String,
) -> Result<TestCloudruResult, String> {
    let base = normalize_fm_base_url(&base_url);
    let has_key_id = !key_id.trim().is_empty();
    let bearer = bearer_for_stt(&key_id, &api_key).await?;
    let raw = api_key.trim().to_string();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;

    let cat = fetch_model_ids(&client, &base, has_key_id, &bearer, &raw).await?;
    let models = cat.stt_model_ids;
    if models.is_empty() && cat.total_in_response == 0 {
        return Ok(TestCloudruResult {
            message: "✓ Ключ подтверждён, но ответ /models пуст — введите id STT вручную (например openai/whisper-large-v3)."
                .to_string(),
            models: vec![],
        });
    }
    if models.is_empty() {
        return Ok(TestCloudruResult {
            message: format!(
                "✓ Соединение успешно. В ответе {} id, STT/аудио по фильтру — ни одного. Укажите id вручную (например openai/whisper-large-v3).",
                cat.total_in_response
            ),
            models: vec![],
        });
    }
    Ok(TestCloudruResult {
        message: format!(
            "✓ Соединение успешно. STT/аудио-моделей: {} (из {} в каталоге).",
            models.len(),
            cat.total_in_response
        ),
        models,
    })
}

#[derive(Serialize, Clone)]
struct TestOpenrouterResult {
    message: String,
    models: Vec<String>,
}

#[tauri::command]
async fn test_openrouter(api_key: String) -> Result<TestOpenrouterResult, String> {
    let key = api_key.trim();
    if key.is_empty() {
        return Err("Пустой API-ключ".into());
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(45))
        .build()
        .map_err(|e| e.to_string())?;
    let response = client
        .get("https://openrouter.ai/api/v1/models")
        .header("Authorization", format!("Bearer {}", key))
        .header("HTTP-Referer", "https://github.com/easystt/easystt")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("OpenRouter {status}: {body}"));
    }
    let json: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    let Some(data) = json.get("data").and_then(|d| d.as_array()) else {
        return Err("Некорректный ответ API: нет поля data".into());
    };
    let mut models: Vec<String> = data
        .iter()
        .filter_map(|e| e.get("id").and_then(|v| v.as_str()).map(String::from))
        .filter(|s| !s.is_empty())
        .collect();
    if models.is_empty() {
        return Ok(TestOpenrouterResult {
            message: "✓ Ключ подтверждён, список /models пуст. Укажите id вручную (например openai/whisper-1)."
                .to_string(),
            models: vec![],
        });
    }
    models.sort();
    Ok(TestOpenrouterResult {
        message: format!("✓ Соединение успешно. Доступно моделей: {}.", models.len()),
        models,
    })
}

#[tauri::command]
async fn cancel_transcription(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    state
        .transcription_cancel
        .store(true, Ordering::SeqCst);
    let _ = app.emit("transcription-cancelled", "Обработка отменена");
    if let Some(ind) = app.get_webview_window("indicator") {
        let _ = ind.hide();
    }
    Ok(())
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
    let local_cache = {
        let s = app.state::<AppState>();
        s.local_whisper.clone()
    };
    let cancel = {
        let s = app.state::<AppState>();
        s.transcription_cancel.clone()
    };
    cancel.store(false, Ordering::SeqCst);

    // Read the saved foreground HWND (captured at PTT press in start_recording).
    let target_hwnd: isize = *state.prev_foreground_hwnd.lock().unwrap();

    tokio::spawn(async move {
        let result = transcribe_with_config(&samples, sample_rate, &config, local_cache, cancel.clone())
            .await;

        if cancel.load(Ordering::SeqCst) {
            let _ = app_clone.emit("transcription-cancelled", "Обработка отменена");
            cancel.store(false, Ordering::SeqCst);
            auto_hide_indicator(&app_clone, 800).await;
            return;
        }

        match &result {
            Ok(text) if !text.is_empty() => {
                let delay = config.inject_delay_ms;
                let method = config.injection_method.clone();
                let restore = config.restore_clipboard;
                let plugins = config.plugins.clone();

                // ── Плагины: перехват текста перед инжектом ──────────────────
                // try_intercept обходит все включённые плагины (таймаут 500 мс каждый).
                // Если хоть один вернул {intercept:true} — текст не вставляем.
                let result = plugin_manager::try_intercept(text, &plugins).await;

                if result.intercepted {
                    // Команда выполнена плагином — отдельное событие для виджета
                    let _ = app_clone.emit("plugin-command", text);
                    let _ = app_clone.emit("transcription-done", text);
                } else {
                    // Restore focus to the window that was active at PTT press.
                    // Windows: use SetForegroundWindow — no widget hide/show needed.
                    // Linux/macOS: skip (global hotkey doesn't steal focus, so the
                    //   target window should still be active in most cases).
                    #[cfg(windows)]
                    if target_hwnd != 0 {
                        use windows::Win32::Foundation::HWND;
                        use windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow;
                        unsafe { let _ = SetForegroundWindow(HWND(target_hwnd as *mut _)); }
                        std::thread::sleep(std::time::Duration::from_millis(80));
                    }
                    #[cfg(not(windows))]
                    {
                        let _ = target_hwnd; // suppress unused warning
                        std::thread::sleep(std::time::Duration::from_millis(50));
                    }

                    let inject_result = inject::inject_text(text, &method, delay, restore);
                    match inject_result {
                        Ok(_) => {
                            let _ = app_clone.emit("transcription-done", text);
                        }
                        Err(e) => {
                            let _ = app_clone.emit("transcription-error", e.to_string());
                        }
                    }
                }
            }
            Err(e) if is_cancelled_msg(&e.to_string()) => {
                let _ = app_clone.emit("transcription-cancelled", e.to_string());
            }
            Ok(_) => {
                let _ = app_clone.emit("transcription-error", "Речь не распознана");
            }
            Err(e) => {
                let _ = app_clone.emit("transcription-error", e.to_string());
            }
        }
        cancel.store(false, Ordering::SeqCst);
        auto_hide_indicator(&app_clone, 1200).await;
    });

    Ok(())
}

async fn transcribe_with_config(
    samples: &[f32],
    sample_rate: u32,
    config: &Config,
    local_cache: Arc<Mutex<Option<LocalWhisperCache>>>,
    cancel: Arc<AtomicBool>,
) -> anyhow::Result<String> {
    let lang = config.language.as_str();
    let effective_lang = if lang == "auto" { "" } else { lang };

    let translate_ru_to_en = config.translate_enabled && config.translate_direction == "ru_to_en";
    let translate_en_to_ru = config.translate_enabled && config.translate_direction == "en_to_ru";

    // ── RU → EN: audio-level Whisper translate task (no LLM needed) ──────────
    // All three backends use Whisper, so we call their translate_audio path.
    // Local: set_translate(true) in whisper-rs; Cloud.ru/OpenRouter: /audio/translations.
    if translate_ru_to_en {
        return match config.stt_backend {
            SttBackend::Local => {
                transcribe_cached(
                    local_cache,
                    path_str(config),
                    samples.to_vec(),
                    sample_rate,
                    effective_lang,
                    cancel,
                    config.local_use_gpu,
                    true, // translate = true
                )
                .await
            }
            SttBackend::Cloudru => {
                let st = CloudRuStt {
                    api_key: config.cloudru_api_key.clone(),
                    key_id: config.cloudru_key_id.clone(),
                    base_url: config.cloudru_base_url.clone(),
                    model: config.cloudru_model.clone(),
                };
                tokio::select! {
                    r = st.translate_audio(samples, sample_rate) => r,
                    _ = until_user_cancel(cancel.clone()) => {
                        Err(anyhow::anyhow!("Обработка отменена"))
                    }
                }
            }
            SttBackend::Openrouter => {
                let st = OpenRouterStt {
                    api_key: config.openrouter_api_key.clone(),
                    model: config.openrouter_model.clone(),
                };
                tokio::select! {
                    r = st.translate_audio(samples, sample_rate) => r,
                    _ = until_user_cancel(cancel.clone()) => {
                        Err(anyhow::anyhow!("Обработка отменена"))
                    }
                }
            }
        };
    }

    // ── Normal transcription (no translation, or EN → RU which needs text LLM) ─
    let raw_text = match config.stt_backend {
        SttBackend::Local => {
            transcribe_cached(
                local_cache,
                path_str(config),
                samples.to_vec(),
                sample_rate,
                effective_lang,
                cancel,
                config.local_use_gpu,
                false,
            )
            .await?
        }
        SttBackend::Cloudru => {
            let st = CloudRuStt {
                api_key: config.cloudru_api_key.clone(),
                key_id: config.cloudru_key_id.clone(),
                base_url: config.cloudru_base_url.clone(),
                model: config.cloudru_model.clone(),
            };
            tokio::select! {
                r = st.transcribe(samples, sample_rate, effective_lang) => r?,
                _ = until_user_cancel(cancel.clone()) => {
                    return Err(anyhow::anyhow!("Обработка отменена"));
                }
            }
        }
        SttBackend::Openrouter => {
            let st = OpenRouterStt {
                api_key: config.openrouter_api_key.clone(),
                model: config.openrouter_model.clone(),
            };
            tokio::select! {
                r = st.transcribe(samples, sample_rate, effective_lang) => r?,
                _ = until_user_cancel(cancel.clone()) => {
                    return Err(anyhow::anyhow!("Обработка отменена"));
                }
            }
        }
    };

    // ── EN → RU: text-level LLM translation (Whisper can't do this at audio level) ─
    if translate_en_to_ru {
        match config.stt_backend {
            SttBackend::Openrouter => {
                return translate_via_openrouter(&raw_text, "en", "ru", &config.openrouter_api_key, &config.translate_model_openrouter).await;
            }
            SttBackend::Cloudru => {
                let bearer = bearer_for_stt(&config.cloudru_key_id, &config.cloudru_api_key)
                    .await
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                let base_url = if config.cloudru_base_url.trim().is_empty() {
                    "https://foundation-models.api.cloud.ru/v1".to_string()
                } else {
                    normalize_fm_base_url(&config.cloudru_base_url)
                };
                return translate_via_cloudru(&raw_text, "en", "ru", &bearer, &base_url, &config.translate_model_cloudru).await;
            }
            SttBackend::Local => {
                return Err(anyhow::anyhow!(
                    "Локальный Whisper поддерживает только перевод RU → EN (бесплатно). \
                     Для EN → RU переключитесь на Cloud.ru или OpenRouter."
                ));
            }
        }
    }

    Ok(raw_text)
}

fn path_str(config: &Config) -> String {
    model_path(&config.local_model_name)
        .to_string_lossy()
        .to_string()
}

// ─── Always-on background listening ──────────────────────────────────────────

fn has_active_plugins(cfg: &Config) -> bool {
    cfg.plugins.iter().any(|p| p.enabled)
}

fn should_start_always_on(cfg: &Config) -> bool {
    cfg.plugins.iter().any(|p| p.enabled)
}

/// Запускает фоновую задачу непрерывного прослушивания, если есть активные плагины.
/// Безопасен для повторного вызова — не запустит вторую задачу, если уже работает.
fn start_always_on_if_needed(app: &AppHandle) {
    let st = match app.try_state::<AppState>() {
        Some(s) => s,
        None => return,
    };
    if !has_active_plugins(&st.config.lock().unwrap()) {
        return;
    }
    if st.always_on_active.swap(true, Ordering::SeqCst) {
        return; // уже запущено
    }
    let device_name = st.config.lock().unwrap().mic_device_name.clone();
    if let Err(e) = st.always_on_recorder.lock().unwrap().start(&device_name) {
        eprintln!("[always-on] не удалось открыть микрофон: {e}");
        st.always_on_active.store(false, Ordering::SeqCst);
        return;
    }
    let samples_arc = st.always_on_recorder.lock().unwrap().samples_arc();
    let rate_arc    = st.always_on_recorder.lock().unwrap().sample_rate_arc();
    let active      = st.always_on_active.clone();
    let local_cache = st.local_whisper.clone();
    let app_clone   = app.clone();

    tauri::async_runtime::spawn(async move {
        while active.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(2000)).await;
            if !active.load(Ordering::SeqCst) {
                break;
            }
            // Пропускаем пока идёт PTT-запись
            {
                let st2 = app_clone.state::<AppState>();
                if st2.recorder.lock().unwrap().is_active() {
                    continue;
                }
            }
            let rate = *rate_arc.lock().unwrap();
            // Снимаем снэпшот последних 5 секунд (скользящее окно)
            let samples: Vec<f32> = {
                let buf = samples_arc.lock().unwrap();
                let max = rate as usize * 5;
                if buf.len() > max { buf[buf.len() - max..].to_vec() } else { buf.clone() }
            };
            // Обрезаем буфер до 7 секунд, не очищая полностью
            {
                let mut buf = samples_arc.lock().unwrap();
                let keep = rate as usize * 7;
                if buf.len() > keep {
                    let drain = buf.len() - keep;
                    buf.drain(..drain);
                }
            }
            if samples.is_empty() {
                continue;
            }
            // VAD: пропускаем тишину
            let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
            if rms < 0.008 {
                continue;
            }
            let st2 = app_clone.state::<AppState>();
            let config = st2.config.lock().unwrap().clone();
            let plugins: Vec<_> = config.plugins.iter().filter(|p| p.enabled).cloned().collect();
            if plugins.is_empty() {
                continue;
            }
            let cancel = Arc::new(AtomicBool::new(false));
            match transcribe_with_config(&samples, rate, &config, local_cache.clone(), cancel).await {
                Ok(text) if !text.is_empty() => {
                    let result = plugin_manager::try_intercept(&text, &plugins).await;
                    if result.intercepted {
                        // Команда выполнена — показываем плашку 2с, потом скрываем
                        samples_arc.lock().unwrap().clear();
                        let _ = app_clone.emit("plugin-command", &text);
                        show_recording_indicator(&app_clone);
                        tokio::time::sleep(Duration::from_millis(2000)).await;
                        if let Some(ind) = app_clone.get_webview_window("indicator") {
                            let _ = ind.hide();
                        }
                    } else if result.agent_detected {
                        // Имя агента сказано, команда не распознана — переходим в PTT
                        samples_arc.lock().unwrap().clear();
                        show_recording_indicator(&app_clone);
                        let _ = app_clone.emit("always-on-vad", ());
                        let _ = app_clone.emit("ptt-pressed", ());
                        tokio::time::sleep(Duration::from_secs(6)).await;
                        let _ = app_clone.emit("ptt-released", ());
                        samples_arc.lock().unwrap().clear();
                    }
                    // Агент не упомянут — ничего не показываем
                }
                _ => {} // Ничего не распознано — молчим
            }
        }
        // Задача завершилась — останавливаем рекордер
        if let Some(st2) = app_clone.try_state::<AppState>() {
            st2.always_on_recorder.lock().unwrap().stop();
        }
    });
}

/// Останавливает фоновое прослушивание если больше нет активных плагинов.
fn stop_always_on_if_needed(app: &AppHandle) {
    let st = match app.try_state::<AppState>() {
        Some(s) => s,
        None => return,
    };
    if has_active_plugins(&st.config.lock().unwrap()) {
        return; // ещё есть активные плагины
    }
    st.always_on_active.store(false, Ordering::SeqCst);
    // Рекордер остановится сам когда задача завершится (видит active=false).
}

// ─── Plugin management commands ───────────────────────────────────────────────

/// Если path — zip-архив, извлекает исполняемый файл рядом с архивом и возвращает его путь.
fn extract_plugin_from_zip(zip_path: &str) -> Result<String, String> {
    let file = std::fs::File::open(zip_path)
        .map_err(|e| format!("Не удалось открыть архив: {e}"))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| format!("Ошибка чтения архива: {e}"))?;

    let out_dir = std::path::Path::new(zip_path)
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .to_path_buf();

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().to_string();
        // Пропускаем директории и вложенные пути
        if name.ends_with('/') || name.contains('/') || name.contains('\\') { continue; }

        #[cfg(windows)]
        let is_exe = name.to_lowercase().ends_with(".exe");
        #[cfg(not(windows))]
        let is_exe = !name.contains('.');

        if !is_exe { continue; }

        let out_path = out_dir.join(&name);
        let mut out_file = std::fs::File::create(&out_path)
            .map_err(|e| format!("Ошибка создания файла «{name}»: {e}"))?;
        std::io::copy(&mut entry, &mut out_file)
            .map_err(|e| format!("Ошибка распаковки «{name}»: {e}"))?;

        #[cfg(not(windows))]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&out_path, std::fs::Permissions::from_mode(0o755));
        }

        return Ok(out_path.to_string_lossy().to_string());
    }

    Err("Исполняемый файл не найден в архиве".to_string())
}

/// Добавить плагин: запускает исполняемый файл, ждёт старта, читает манифест.
#[tauri::command]
async fn add_plugin(
    app: AppHandle,
    path: String,
    port: u16,
    state: State<'_, AppState>,
) -> Result<PluginEntry, String> {
    // Если передан zip-архив — распаковываем сначала
    let path = if path.to_lowercase().ends_with(".zip") {
        extract_plugin_from_zip(&path)?
    } else {
        path
    };

    if !std::path::Path::new(&path).exists() {
        return Err(format!("Файл не найден: {path}"));
    }

    // Генерируем id из времени (без uuid-зависимости)
    let id = format!(
        "{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );

    // Создаём временную запись чтобы передать в spawn
    let tmp_entry = PluginEntry {
        id: id.clone(),
        path: path.clone(),
        enabled: true,
        name: String::new(),
        version: String::new(),
        port,
    };

    state
        .plugin_manager
        .spawn(&tmp_entry)
        .map_err(|e| e.to_string())?;

    // Ждём запуска сервера (до 5 попыток по 1 с)
    let mut manifest = serde_json::Value::Null;
    for _ in 0..5 {
        tokio::time::sleep(Duration::from_secs(1)).await;
        if let Ok(m) = plugin_manager::fetch_manifest(port).await {
            manifest = m;
            break;
        }
    }

    let name = manifest
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| {
            std::path::Path::new(&path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Плагин")
        })
        .to_string();
    let version = manifest
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("?")
        .to_string();

    let entry = PluginEntry {
        id,
        path,
        enabled: true,
        name,
        version,
        port,
    };

    state.config.lock().unwrap().plugins.push(entry.clone());
    start_always_on_if_needed(&app);
    Ok(entry)
}

/// Удалить плагин: сбрасывает его конфиг через /reset, убивает процесс, удаляет из конфига easySTT.
#[tauri::command]
async fn remove_plugin(app: AppHandle, id: String, state: State<'_, AppState>) -> Result<(), String> {
    let port = state.config.lock().unwrap()
        .plugins.iter().find(|p| p.id == id).map(|p| p.port);
    // Просим плагин удалить свой конфиг и выйти
    if let Some(port) = port {
        let _ = plugin_manager::reset_plugin(port).await;
        tokio::time::sleep(Duration::from_millis(400)).await;
    }
    state.plugin_manager.kill(&id);
    state.config.lock().unwrap().plugins.retain(|p| p.id != id);
    stop_always_on_if_needed(&app);
    Ok(())
}

/// Включить / выключить плагин.
#[tauri::command]
fn toggle_plugin(app: AppHandle, id: String, enabled: bool, state: State<'_, AppState>) -> Result<(), String> {
    {
        let mut cfg = state.config.lock().unwrap();
        if let Some(p) = cfg.plugins.iter_mut().find(|p| p.id == id) {
            p.enabled = enabled;
        }
    }
    if enabled {
        let entry = state.config.lock().unwrap().plugins.iter().find(|p| p.id == id).cloned();
        if let Some(e) = entry {
            let _ = state.plugin_manager.spawn(&e);
        }
        start_always_on_if_needed(&app);
    } else {
        state.plugin_manager.kill(&id);
        stop_always_on_if_needed(&app);
    }
    Ok(())
}

/// Открыть окно настроек плагина через HTTP /open-settings.
#[tauri::command]
async fn open_plugin_settings(id: String, state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    let port = state
        .config
        .lock()
        .unwrap()
        .plugins
        .iter()
        .find(|p| p.id == id)
        .map(|p| p.port)
        .ok_or("Плагин не найден")?;

    if !state.plugin_manager.is_running(&id) {
        return Err(
            "Плагин не запущен. Проверьте, что добавлен standalone-бинарник (target/release/), а не установщик.".into()
        );
    }

    plugin_manager::open_settings(port)
        .await
        .map_err(|e| format!("Не удалось открыть настройки: {e}"))?;

    // Снимаем always-on-top с окна настроек easySTT, чтобы плагин мог выйти поверх.
    // При следующем открытии настроек show_settings_raised снова поднимет его.
    if let Some(s) = app.get_webview_window("settings") {
        let _ = s.set_always_on_top(false);
    }
    Ok(())
}

/// Статус каждого плагина: id → running.
#[tauri::command]
fn get_plugins_status(
    state: State<'_, AppState>,
) -> std::collections::HashMap<String, bool> {
    let cfg = state.config.lock().unwrap();
    cfg.plugins
        .iter()
        .map(|p| (p.id.clone(), state.plugin_manager.is_running(&p.id)))
        .collect()
}

#[tauri::command]
fn get_model_catalog() -> Vec<ModelInfo> {
    model_catalog()
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DownloadProgress {
    model_id: String,
    downloaded_bytes: u64,
    total_bytes: u64,
    percent: f32,
}

#[tauri::command]
async fn download_model(
    app: AppHandle,
    state: State<'_, AppState>,
    model_id: String,
) -> Result<(), String> {
    let catalog = model_catalog();
    let info = catalog
        .iter()
        .find(|m| m.id == model_id)
        .ok_or_else(|| format!("Модель «{model_id}» не найдена в каталоге"))?
        .clone();

    let cancel = state.download_cancel.clone();
    cancel.store(false, Ordering::SeqCst);

    let dest_path = model_path(&model_id);
    if let Some(parent) = dest_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Не удалось создать папку моделей: {e}"))?;
    }

    // Temp file во избежание частично скачанного файла при отмене/ошибке.
    let tmp_path = dest_path.with_extension("bin.tmp");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3600))
        .build()
        .map_err(|e| e.to_string())?;

    let response = client
        .get(&info.url)
        .send()
        .await
        .map_err(|e| format!("Ошибка соединения: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("Сервер вернул {}: проверьте доступ к HuggingFace", response.status()));
    }

    let total_bytes = response.content_length().unwrap_or(info.size_mb * 1_024 * 1_024);

    use tokio::io::AsyncWriteExt;
    let mut file = tokio::fs::File::create(&tmp_path)
        .await
        .map_err(|e| format!("Ошибка создания файла: {e}"))?;

    let mut downloaded: u64 = 0;
    let mut last_reported: u64 = 0;
    let report_every: u64 = 1_024 * 1_024; // каждые 1 МБ

    let mut stream = response.bytes_stream();
    use futures_util::StreamExt;
    loop {
        if cancel.load(Ordering::SeqCst) {
            drop(file);
            let _ = tokio::fs::remove_file(&tmp_path).await;
            return Err("Загрузка отменена".into());
        }
        match stream.next().await {
            Some(Ok(chunk)) => {
                file.write_all(&chunk)
                    .await
                    .map_err(|e| format!("Ошибка записи: {e}"))?;
                downloaded += chunk.len() as u64;
                if downloaded - last_reported >= report_every {
                    last_reported = downloaded;
                    let percent = if total_bytes > 0 {
                        (downloaded as f32 / total_bytes as f32 * 100.0).min(100.0)
                    } else {
                        0.0
                    };
                    let _ = app.emit("model-download-progress", DownloadProgress {
                        model_id: model_id.clone(),
                        downloaded_bytes: downloaded,
                        total_bytes,
                        percent,
                    });
                }
            }
            Some(Err(e)) => {
                drop(file);
                let _ = tokio::fs::remove_file(&tmp_path).await;
                return Err(format!("Ошибка при загрузке: {e}"));
            }
            None => break,
        }
    }

    file.flush().await.map_err(|e| format!("Ошибка сброса буфера: {e}"))?;
    drop(file);

    tokio::fs::rename(&tmp_path, &dest_path)
        .await
        .map_err(|e| format!("Ошибка сохранения файла: {e}"))?;

    let _ = app.emit("model-download-progress", DownloadProgress {
        model_id: model_id.clone(),
        downloaded_bytes: downloaded,
        total_bytes: downloaded,
        percent: 100.0,
    });
    let _ = app.emit("model-download-done", &model_id);
    Ok(())
}

#[tauri::command]
fn cancel_model_download(state: State<'_, AppState>) {
    state.download_cancel.store(true, Ordering::SeqCst);
}

#[tauri::command]
fn is_autostart_launch(state: State<'_, AppState>) -> bool {
    state.is_autostart
}

#[tauri::command]
fn get_models_dir() -> String {
    models_dir().to_string_lossy().to_string()
}

#[tauri::command]
fn delete_model(name: String, state: State<'_, AppState>) -> Result<(), String> {
    let path = model_path(&name);
    if !path.exists() {
        return Ok(()); // уже удалена
    }
    // Если эта модель сейчас загружена — сбрасываем кэш
    if let Ok(mut cache) = state.local_whisper.lock() {
        if cache.as_ref().map(|c| c.model_path == path.to_string_lossy().as_ref()).unwrap_or(false) {
            *cache = None;
        }
    }
    std::fs::remove_file(&path).map_err(|e| format!("Ошибка удаления: {e}"))
}

#[tauri::command]
fn apply_config(config: Config, state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    let prev = state.config.lock().unwrap().clone();
    let old_hotkey = prev.hotkey.clone();
    let new_hotkey = config.hotkey.clone();
    #[cfg(not(target_os = "linux"))]
    let new_bg = config.widget_bg_to.clone();

    if prev.stt_backend != config.stt_backend
        || (config.stt_backend == SttBackend::Local
            && (prev.local_model_name != config.local_model_name
                || prev.local_use_gpu != config.local_use_gpu))
    {
        *state.local_whisper.lock().unwrap() = None;
    }

    *state.config.lock().unwrap() = config;

    if old_hotkey != new_hotkey {
        register_hotkey(&app, &new_hotkey);
    }
    // Синхронизируем нативный фон окна виджета с выбранным цветом темы.
    // На Linux окно прозрачное — не трогаем фон, иначе перекроем прозрачность.
    if let Some(_w) = app.get_webview_window("widget") {
        #[cfg(not(target_os = "linux"))]
        if let Some(color) = parse_hex_color(&new_bg) {
            let _ = _w.set_background_color(Some(color));
        }
        #[cfg(windows)]
        {
            let corner = state.config.lock().unwrap().widget_corner_style.clone();
            win_widget::apply_win32_widget_region(&_w, &corner);
        }
    }
    {
        let cfg = state.config.lock().unwrap().clone();
        for plugin in cfg.plugins.iter().filter(|p| p.enabled) {
            let _ = state.plugin_manager.spawn(plugin);
        }
        if should_start_always_on(&cfg) {
            start_always_on_if_needed(&app);
        } else {
            state.always_on_active.store(false, Ordering::SeqCst);
        }
    }
    let _ = app.emit("config-updated", json!({}));
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

/// Показать окно настроек поверх always-on-top виджета, снять минимизацию (после свернуть-развернуть ОС/трей).
fn show_settings_raised(app: &AppHandle) -> Result<(), String> {
    let settings = app
        .get_webview_window("settings")
        .ok_or("Окно настроек не найдено")?;
    if let Some(widget) = app.get_webview_window("widget") {
        let _ = widget.set_always_on_top(false);
        let _ = widget.hide();
    }
    let _ = settings.set_always_on_top(true);
    settings
        .unminimize()
        .map_err(|e| e.to_string())?;
    settings.show().map_err(|e| e.to_string())?;
    settings.set_focus().map_err(|e| e.to_string())?;
    Ok(())
}

/// Позиционирует и показывает индикатор записи в левом нижнем углу экрана.
fn show_recording_indicator(app: &AppHandle) {
    if let Some(ind) = app.get_webview_window("indicator") {
        if let Ok(Some(mon)) = ind.primary_monitor() {
            let sf     = mon.scale_factor();
            let mpos   = mon.position();
            let margin = (16.0 * sf) as i32;
            let win_h  = (44.0 * sf) as i32;
            let x = mpos.x + margin;
            #[cfg(windows)]
            let bottom = crate::win_widget::work_area_bottom_px();
            #[cfg(not(windows))]
            let bottom = mpos.y + mon.size().height as i32;
            let y = bottom - win_h - margin;
            let _ = ind.set_position(tauri::PhysicalPosition::new(x, y));
        }
        let _ = ind.set_always_on_top(true);
        let _ = ind.show();
    }
}

/// После скрытия настроек — снова показать виджет и закрепить его «поверх окон».
fn apply_settings_closed(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("widget") {
        let _ = w.show();
        let _ = w.set_always_on_top(true);
    }
    if let Some(s) = app.get_webview_window("settings") {
        let _ = s.set_always_on_top(false);
    }
}

#[tauri::command]
fn open_settings(app: AppHandle) -> Result<(), String> {
    show_settings_raised(&app)
}

#[tauri::command]
fn minimize_widget(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("widget") {
        win.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}

// ─── Hotkey Registration ─────────────────────────────────────────────────────

fn register_hotkey(app: &AppHandle, hotkey: &str) {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let shortcut: tauri_plugin_global_shortcut::Shortcut = match hotkey.parse() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Invalid hotkey '{hotkey}': {e}");
            return;
        }
    };

    let app_clone = app.clone();
    let _ = gs.on_shortcut(shortcut, move |_app, _shortcut, event| {
        match event.state() {
            ShortcutState::Pressed => {
                show_recording_indicator(&app_clone);
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
    let mut tray = TrayIconBuilder::new().menu(&menu).tooltip("easySTT");
    if let Some(icon) = app.default_window_icon().cloned() {
        tray = tray.icon(icon);
    }
    tray.on_menu_event(|app, event| match event.id.as_ref() {
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
                let _ = show_settings_raised(&app);
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
        local_whisper: Arc::new(Mutex::new(None)),
        transcription_cancel: Arc::new(AtomicBool::new(false)),
        download_cancel: Arc::new(AtomicBool::new(false)),
        prev_foreground_hwnd: Mutex::new(0),
        is_autostart: std::env::args().any(|a| a == "--autostart"),
        plugin_manager: plugin_manager::PluginManager::new(),
        always_on_recorder: Mutex::new(AudioRecorder::new()),
        always_on_active: Arc::new(AtomicBool::new(false)),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // Вторая попытка запуска — показываем уже работающий виджет.
            if let Some(w) = app.get_webview_window("widget") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--autostart"]),
        ))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .manage(state)
        .setup(|app| {
            setup_tray(app)?;
            let app_handle = app.handle().clone();
            // Устанавливаем нативный фон окна = сохранённый widgetBgTo (или дефолт).
            // Это синхронизирует угловые пиксели (за CSS border-radius) с цветом виджета.
            // Окно стартует скрытым (visible: false в tauri.conf.json) — React покажет его
            // сам после применения цветов, чтобы избежать чёрного кадра при инициализации.
            #[allow(unused_variables)]
            if let Some(w) = app.get_webview_window("widget") {
                // Должно совпадать с store до вызова apply_config из UI: иначе при первом
                // `WindowEvent::Resized` обработчик вызовет apply с дефолтом `none` и сбросит
                // `SetWindowRgn` (скругление HWND) после перезапуска.
                let mut corner_style = "none".to_string();
                #[cfg(not(target_os = "linux"))]
                let mut bg = WIDGET_BG;
                if let Ok(store) = app.store("settings.json") {
                    #[cfg(not(target_os = "linux"))]
                    if let Some(hex) = store
                        .get("widgetBgTo")
                        .and_then(|v| v.as_str().map(String::from))
                    {
                        bg = parse_hex_color(&hex).unwrap_or(WIDGET_BG);
                    }
                    if let Some(s) = store
                        .get("widgetCornerStyle")
                        .and_then(|v| v.as_str().map(String::from))
                    {
                        if s == "round" {
                            corner_style = "round".to_string();
                        } else {
                            corner_style = "none".to_string();
                        }
                    }
                }
                if let Some(st) = app.handle().try_state::<AppState>() {
                    st.config
                        .lock()
                        .unwrap()
                        .widget_corner_style
                        .clone_from(&corner_style);
                }
                // На Linux окно прозрачное (tauri.linux.conf.json transparent:true) —
                // не устанавливаем сплошной фон, иначе перекроем прозрачность углов.
                #[cfg(not(target_os = "linux"))]
                let _ = w.set_background_color(Some(bg));
                #[cfg(windows)]
                {
                    win_widget::apply_win32_widget_region(&w, &corner_style);
                    let w_ev = w.clone();
                    let ah = app_handle.clone();
                    w_ev.on_window_event(move |e| {
                        match e {
                            tauri::WindowEvent::Resized(_)
                            | tauri::WindowEvent::ScaleFactorChanged { .. } => {
                                if let Some(st) = ah.try_state::<AppState>() {
                                    let c = st.config.lock().unwrap().widget_corner_style.clone();
                                    if let Some(widget) = ah.get_webview_window("widget") {
                                        win_widget::apply_win32_widget_region(&widget, &c);
                                    }
                                }
                            }
                            _ => {}
                        }
                    });
                }
            }
            // Linux: окно visible:true по умолчанию (tauri.linux.conf.json).
            // При автозапуске (--autostart) виджет должен оставаться скрытым — только трей.
            // Иначе показываем с небольшой задержкой на случай гонки рендера.
            #[cfg(target_os = "linux")]
            {
                let ah = app_handle.clone();
                let autostart = app.state::<AppState>().is_autostart;
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                    if let Some(w) = ah.get_webview_window("widget") {
                        if autostart {
                            let _ = w.hide();
                        } else {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                });
            }

            // Принудительно выставляем always-on-top для виджета при старте
            // (конфиг tauri.conf.json устанавливает его, но явный вызов надёжнее на Windows).
            if let Some(w) = app.get_webview_window("widget") {
                let _ = w.set_always_on_top(true);
            }

            let config = app.state::<AppState>().config.lock().unwrap().clone();
            register_hotkey(&app_handle, &config.hotkey);

            // Авто-запуск включённых плагинов + фоновое прослушивание
            {
                let st = app.state::<AppState>();
                for plugin in config.plugins.iter().filter(|p| p.enabled) {
                    if let Err(e) = st.plugin_manager.spawn(plugin) {
                        eprintln!("[plugins] Не удалось запустить «{}»: {e}", plugin.name);
                    }
                }
            }
            start_always_on_if_needed(&app_handle);

            agent_api::start(&app_handle);
            if let Some(settings_win) = app.get_webview_window("settings") {
                let win = settings_win.clone();
                let ah = app_handle.clone();
                settings_win.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = win.hide();
                        apply_settings_closed(&ah);
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start_recording,
            stop_and_transcribe,
            cancel_transcription,
            apply_config,
            get_model_path,
            model_exists,
            is_autostart_launch,
            get_model_catalog,
            download_model,
            cancel_model_download,
            get_models_dir,
            delete_model,
            open_settings,
            minimize_widget,
            quit_app,
            list_microphones,
            get_local_accel_info,
            get_local_stt_build_info,
            test_cloudru,
            test_openrouter,
            add_plugin,
            remove_plugin,
            toggle_plugin,
            open_plugin_settings,
            get_plugins_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running easySTT");
}
