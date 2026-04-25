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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub stt_backend: SttBackend,
    pub language: String,
    pub injection_method: InjectionMethod,
    pub hotkey: String,
    pub local_model_name: String,
    pub cloudru_api_key: String,
    pub cloudru_base_url: String,
    pub openrouter_api_key: String,
    pub openrouter_model: String,
    pub inject_delay_ms: u64,
    pub restore_clipboard: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            stt_backend: SttBackend::Cloudru,
            language: "ru".into(),
            injection_method: InjectionMethod::Clipboard,
            hotkey: "Alt+`".into(),
            local_model_name: "base".into(),
            cloudru_api_key: String::new(),
            cloudru_base_url: "https://foundation-models.api.cloud.ru/v1".into(),
            openrouter_api_key: String::new(),
            openrouter_model: "openai/whisper-1".into(),
            inject_delay_ms: 150,
            restore_clipboard: false,
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
