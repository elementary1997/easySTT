use super::model_filter::is_stt_catalog_entry;
use super::SttBackend;
use crate::audio::{pcm_to_wav, resample_to_16k};
use async_trait::async_trait;
use reqwest::multipart;
use reqwest::Client;
use reqwest::StatusCode;
use serde_json::Value;

pub struct CloudRuStt {
    pub api_key: String,
    pub key_id: String,
    pub base_url: String,
    /// OpenAI-совместимый id, например `openai/whisper-large-v3`
    pub model: String,
}

const IAM_TOKEN_URL: &str = "https://iam.api.cloud.ru/api/v1/auth/token";
/// Значение по умолчанию, если в настройках не задана модель.
pub const DEFAULT_CLOUDRU_MODEL: &str = "openai/whisper-large-v3";

/// Strips trailing slashes for joining paths.
pub fn normalize_fm_base_url(s: &str) -> String {
    s.trim().trim_end_matches('/').to_string()
}

/// - Только **Secret** → `Bearer` (как в OpenAI SDK) или, если API вернёт 401, повтор с `Api-Key`
/// - **Key ID** + **Secret** → IAM, затем только `Bearer` с `access_token`
pub async fn bearer_for_stt(cloudru_key_id: &str, cloudru_api_key: &str) -> Result<String, String> {
    let key_id = cloudru_key_id.trim();
    let secret = cloudru_api_key.trim();
    if secret.is_empty() {
        return Err("Укажите API ключ (Key Secret) Cloud.ru в настройках".into());
    }
    if key_id.is_empty() {
        return Ok(secret.to_string());
    }

    let body = serde_json::json!({
        "keyId": key_id,
        "secret": secret,
    });

    let response = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?
        .post(IAM_TOKEN_URL)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("IAM: нет соединения: {e}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let t = response.text().await.unwrap_or_default();
        return Err(format!("IAM: HTTP {status} — {t}"));
    }

    let v: Value = response.json().await.map_err(|e| e.to_string())?;
    extract_access_token(&v)
        .ok_or_else(|| "IAM: в ответе нет access_token. Проверьте Key ID и Key Secret".into())
}

fn extract_access_token(v: &Value) -> Option<String> {
    v.get("access_token")
        .or_else(|| v.get("accessToken"))
        .and_then(|t| t.as_str())
        .map(str::to_owned)
        .or_else(|| {
            v.get("result")
                .and_then(|r| r.get("access_token").or_else(|| r.get("accessToken")))
                .and_then(|t| t.as_str().map(String::from))
        })
}

fn build_transcribe_form(
    model_id: &str,
    wav: Vec<u8>,
    language: &str,
) -> Result<multipart::Form, anyhow::Error> {
    let file_part = multipart::Part::bytes(wav)
        .file_name("audio.wav")
        .mime_str("audio/wav")?;
    let mut form = multipart::Form::new()
        .text("model", model_id.to_string())
        .text("response_format", "json")
        .part("file", file_part);
    if !language.is_empty() && language != "auto" {
        form = form.text("language", language.to_owned());
    }
    Ok(form)
}

/// POST to `/audio/transcriptions` or `/audio/translations` depending on `endpoint`.
async fn post_audio_endpoint(
    endpoint: &str, // "transcriptions" | "translations"
    model_id: &str,
    client: &Client,
    base: &str,
    wav: Vec<u8>,
    language: &str, // ignored for translations (Whisper auto-detects)
    has_key_id: bool,
    auth_bearer: &str,
    raw_secret: &str,
) -> Result<reqwest::Response, anyhow::Error> {
    let url = format!("{base}/audio/{endpoint}");
    let wav_retry = wav.clone();
    let form = build_transcribe_form(model_id, wav, if endpoint == "translations" { "" } else { language })?;
    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {auth_bearer}"))
        .multipart(form)
        .send()
        .await?;

    if response.status() == StatusCode::UNAUTHORIZED && !has_key_id && !raw_secret.is_empty() {
        let form2 = build_transcribe_form(model_id, wav_retry, if endpoint == "translations" { "" } else { language })?;
        return Ok(client
            .post(&url)
            .header("Authorization", format!("Api-Key {raw_secret}"))
            .multipart(form2)
            .send()
            .await?);
    }
    Ok(response)
}

/// POST: сначала `Authorization: Bearer …`; если 401 и это не IAM-flow — второй с `Api-Key` (тот же секрет).
pub async fn post_audio_transcribe(
    model_id: &str,
    client: &Client,
    base: &str,
    wav: Vec<u8>,
    language: &str,
    has_key_id: bool,
    auth_bearer: &str,
    raw_secret: &str,
) -> Result<reqwest::Response, anyhow::Error> {
    post_audio_transcribe_impl(
        model_id,
        client,
        base,
        wav,
        language,
        has_key_id,
        auth_bearer,
        raw_secret,
    )
    .await
}

async fn post_audio_transcribe_impl(
    model_id: &str,
    client: &Client,
    base: &str,
    wav: Vec<u8>,
    language: &str,
    has_key_id: bool,
    auth_bearer: &str,
    raw_secret: &str,
) -> Result<reqwest::Response, anyhow::Error> {
    post_audio_endpoint(
        "transcriptions",
        model_id,
        client,
        base,
        wav,
        language,
        has_key_id,
        auth_bearer,
        raw_secret,
    )
    .await
}

/// Сводка: только STT/аудио, весь `data` для подсказок.
pub struct SttCatalogList {
    pub stt_model_ids: Vec<String>,
    pub total_in_response: usize,
}

/// GET `/v1/models` — только id, подходящие для `audio/transcriptions` (whisper, transcribe, …).
pub async fn fetch_model_ids(
    client: &Client,
    base: &str,
    has_key_id: bool,
    auth_bearer: &str,
    raw_secret: &str,
) -> Result<SttCatalogList, String> {
    let url = format!("{base}/models");
    let r1 = client
        .get(&url)
        .header("Authorization", format!("Bearer {auth_bearer}"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let r1 = if r1.status() == StatusCode::UNAUTHORIZED && !has_key_id && !raw_secret.is_empty() {
        client
            .get(&url)
            .header("Authorization", format!("Api-Key {raw_secret}"))
            .send()
            .await
            .map_err(|e| e.to_string())?
    } else {
        r1
    };
    if !r1.status().is_success() {
        let s = r1.status();
        let t = r1.text().await.unwrap_or_default();
        return Err(format!("GET /models → HTTP {s}: {t}"));
    }
    let v: Value = r1.json().await.map_err(|e| e.to_string())?;
    let arr = v
        .get("data")
        .and_then(|d| d.as_array())
        .or_else(|| {
            v.get("result")
                .and_then(|r| r.get("data"))
                .and_then(|d| d.as_array())
        })
        .ok_or_else(|| "Ответ /models: ожидается data[] (OpenAI-совместимый формат)".to_string())?;
    let total_in_response = arr.len();
    let mut ids: Vec<String> = arr
        .iter()
        .filter(|m| is_stt_catalog_entry(m))
        .filter_map(|m| m.get("id").and_then(|i| i.as_str()).map(String::from))
        .filter(|s| !s.is_empty())
        .collect();
    if ids.is_empty() {
        return Ok(SttCatalogList {
            stt_model_ids: vec![],
            total_in_response,
        });
    }
    ids.sort_by(|a, b| {
        let a_stt = a.to_lowercase();
        let b_stt = b.to_lowercase();
        let a_pri = a_stt.contains("whisper") as i32
            + a_stt.contains("transcrib") as i32
            + a_stt.contains("speech") as i32;
        let b_pri = b_stt.contains("whisper") as i32
            + b_stt.contains("transcrib") as i32
            + b_stt.contains("speech") as i32;
        b_pri.cmp(&a_pri).then_with(|| a.cmp(b))
    });
    Ok(SttCatalogList {
        stt_model_ids: ids,
        total_in_response,
    })
}

#[async_trait]
impl SttBackend for CloudRuStt {
    async fn transcribe(
        &self,
        samples: &[f32],
        sample_rate: u32,
        language: &str,
    ) -> anyhow::Result<String> {
        let resampled = resample_to_16k(samples, sample_rate);
        let wav = pcm_to_wav(&resampled, 16000);
        let has_key_id = !self.key_id.trim().is_empty();
        let bearer = bearer_for_stt(&self.key_id, &self.api_key)
            .await
            .map_err(anyhow::Error::msg)?;
        let raw = self.api_key.trim().to_string();
        let base = normalize_fm_base_url(&self.base_url);
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()?;
        let model_id = self
            .model
            .trim();
        let model_id = if model_id.is_empty() {
            DEFAULT_CLOUDRU_MODEL
        } else {
            model_id
        };
        let response = post_audio_transcribe(
            model_id,
            &client,
            &base,
            wav,
            language,
            has_key_id,
            &bearer,
            &raw,
        )
        .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Cloud.ru API {status}: {body}"));
        }

        let json: Value = response.json().await?;
        Ok(json["text"]
            .as_str()
            .unwrap_or("")
            .trim()
            .to_string())
    }
}

impl CloudRuStt {
    /// Use Whisper's built-in translate task via `/audio/translations` (audio → English text).
    pub async fn translate_audio(&self, samples: &[f32], sample_rate: u32) -> anyhow::Result<String> {
        let resampled = resample_to_16k(samples, sample_rate);
        let wav = pcm_to_wav(&resampled, 16000);
        let has_key_id = !self.key_id.trim().is_empty();
        let bearer = bearer_for_stt(&self.key_id, &self.api_key)
            .await
            .map_err(anyhow::Error::msg)?;
        let raw = self.api_key.trim().to_string();
        let base = normalize_fm_base_url(&self.base_url);
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()?;
        let model_id = self.model.trim();
        let model_id = if model_id.is_empty() { DEFAULT_CLOUDRU_MODEL } else { model_id };

        let response = post_audio_endpoint(
            "translations",
            model_id,
            &client,
            &base,
            wav,
            "",
            has_key_id,
            &bearer,
            &raw,
        )
        .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Cloud.ru translate {status}: {body}"));
        }

        let json: Value = response.json().await?;
        Ok(json["text"].as_str().unwrap_or("").trim().to_string())
    }
}
