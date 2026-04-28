use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum SttBackend {
    Local,
    Cloudru,
    Openrouter,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum InjectionMethod {
    Clipboard,
    Typing,
}

fn def_widget_bg_from() -> String {
    "#12121f".to_string()
}
fn def_widget_bg_to() -> String {
    "#1a1a2e".to_string()
}
fn def_widget_w1() -> String {
    "#4c84ff".to_string()
}
fn def_widget_w2() -> String {
    "#c762ff".to_string()
}
fn def_widget_w3() -> String {
    "#ff65a7".to_string()
}
fn def_cloudru_model() -> String {
    "openai/whisper-large-v3".to_string()
}
fn def_widget_animation() -> String {
    "flow".to_string()
}
fn def_widget_wave_form() -> String {
    "rolling".to_string()
}
fn def_widget_corner_style() -> String {
    "none".to_string()
}
fn def_translate_direction() -> String {
    "ru_to_en".to_string()
}
fn def_translate_model_cloudru() -> String {
    "Qwen/Qwen2.5-72B-Instruct".to_string()
}
fn def_translate_model_openrouter() -> String {
    "openai/gpt-4o-mini".to_string()
}

/// Перевод транскрипта в URL-открытие на стороннем AI-чате (Perplexity, ChatGPT, Claude…).
/// `url_template` должен содержать плейсхолдер `{q}` — туда подставляется URL-кодированный текст.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AiTarget {
    pub id: String,
    pub name: String,
    pub url_template: String,
    pub hotkey: String,
    pub enabled: bool,
}

fn def_ai_targets() -> Vec<AiTarget> {
    vec![
        AiTarget {
            id: "perplexity".into(),
            name: "Perplexity".into(),
            url_template: "https://www.perplexity.ai/search?q={q}".into(),
            hotkey: "Alt+Shift+1".into(),
            enabled: true,
        },
        AiTarget {
            id: "chatgpt".into(),
            name: "ChatGPT".into(),
            url_template: "https://chatgpt.com/?q={q}".into(),
            hotkey: "Alt+Shift+2".into(),
            enabled: true,
        },
        AiTarget {
            id: "claude".into(),
            name: "Claude".into(),
            url_template: "https://claude.ai/new?q={q}".into(),
            hotkey: "Alt+Shift+3".into(),
            enabled: true,
        },
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub stt_backend: SttBackend,
    pub language: String,
    pub injection_method: InjectionMethod,
    pub hotkey: String,
    pub local_model_name: String,
    pub local_use_gpu: bool,
    pub cloudru_api_key: String,
    /// Key ID (логин) из консоли Cloud.ru — вместе с [Self::cloudru_api_key] (Key Secret) для получения IAM-токена; в UI не показываем
    pub cloudru_key_id: String,
    pub cloudru_base_url: String,
    /// Id модели Foundation Models (из GET /v1/models), напр. `openai/whisper-large-v3`
    #[serde(default = "def_cloudru_model")]
    pub cloudru_model: String,
    pub openrouter_api_key: String,
    pub openrouter_model: String,
    pub inject_delay_ms: u64,
    pub restore_clipboard: bool,
    pub mic_device_name: String,
    // Виджет: фон + три цвета волны/градиента (hex, например #1a1a2e)
    #[serde(default = "def_widget_bg_from")]
    pub widget_bg_from: String,
    #[serde(default = "def_widget_bg_to")]
    pub widget_bg_to: String,
    #[serde(default = "def_widget_w1")]
    pub widget_wave_1: String,
    #[serde(default = "def_widget_w2")]
    pub widget_wave_2: String,
    #[serde(default = "def_widget_w3")]
    pub widget_wave_3: String,
    /// flow | breathe | static
    #[serde(default = "def_widget_animation")]
    pub widget_animation: String,
    /// rolling | smooth | line
    #[serde(default = "def_widget_wave_form")]
    pub widget_wave_form: String,
    /// none | round
    #[serde(default = "def_widget_corner_style")]
    pub widget_corner_style: String,

    // ── Real-time translation ────────────────────────────────────────────
    /// Translate transcribed text before injection.
    #[serde(default)]
    pub translate_enabled: bool,
    /// Translation direction: "ru_to_en" | "en_to_ru".
    #[serde(default = "def_translate_direction")]
    pub translate_direction: String,
    /// LLM model used for translation on Cloud.ru (chat completions).
    #[serde(default = "def_translate_model_cloudru")]
    pub translate_model_cloudru: String,
    /// LLM model used for translation on OpenRouter (chat completions).
    #[serde(default = "def_translate_model_openrouter")]
    pub translate_model_openrouter: String,

    /// Список целей AI-режима. Каждая регистрирует свой глобальный хоткей; при срабатывании
    /// транскрипт уходит не в активное окно через inject, а открывается в браузере по URL-шаблону.
    #[serde(default = "def_ai_targets")]
    pub ai_targets: Vec<AiTarget>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            stt_backend: SttBackend::Cloudru,
            language: "auto".into(),
            injection_method: InjectionMethod::Clipboard,
            hotkey: "Alt+`".into(),
            local_model_name: "tiny".into(),
            local_use_gpu: true,
            cloudru_api_key: String::new(),
            cloudru_key_id: String::new(),
            cloudru_base_url: "https://foundation-models.api.cloud.ru/v1".into(),
            cloudru_model: "openai/whisper-large-v3".into(),
            openrouter_api_key: String::new(),
            openrouter_model: "openai/whisper-1".into(),
            inject_delay_ms: 150,
            restore_clipboard: false,
            mic_device_name: String::new(),
            widget_bg_from: "#12121f".into(),
            widget_bg_to: "#1a1a2e".into(),
            widget_wave_1: "#4c84ff".into(),
            widget_wave_2: "#c762ff".into(),
            widget_wave_3: "#ff65a7".into(),
            widget_animation: "flow".into(),
            widget_wave_form: "rolling".into(),
            widget_corner_style: "none".into(),
            translate_enabled: false,
            translate_direction: "ru_to_en".into(),
            translate_model_cloudru: "Qwen/Qwen2.5-72B-Instruct".into(),
            translate_model_openrouter: "openai/gpt-4o-mini".into(),
            ai_targets: def_ai_targets(),
        }
    }
}

pub fn models_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("easystt")
        .join("models")
}

pub fn model_path(name: &str) -> PathBuf {
    models_dir().join(format!("ggml-{}.bin", name))
}
