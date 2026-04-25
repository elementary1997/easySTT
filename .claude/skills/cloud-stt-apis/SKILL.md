---
name: cloud-stt-apis
description: Cloud STT API integration for easySTT — Cloud.ru Foundation Models (Whisper-large-v3) and OpenRouter multimodal audio. Covers authentication, request format (multipart WAV upload), language parameter, error handling. Use when working on cloudru.rs or openrouter.rs backends.
version: 1.0.0
---

# Cloud STT API Integration

## Cloud.ru Foundation Models — Whisper-large-v3

Cloud.ru provides Whisper-large-v3 via their Foundation Models API.
Their API is OpenAI-compatible — same endpoint format as OpenAI's audio transcription.

### Authentication

```rust
// API key: Cloud.ru console → Users → Service Accounts → Create API key
// Select service: Foundation Models, set validity period (1 day - 1 year)
// Save the Key Secret immediately — it cannot be retrieved afterwards!
//
// Base URL: https://foundation-models.api.cloud.ru/v1
// Endpoint: POST /audio/transcriptions  (OpenAI-compatible)
// Auth header: Authorization: Bearer <api_key>
```

### cloudru.rs Implementation

```rust
// src-tauri/src/stt/cloudru.rs
use reqwest::multipart;

pub struct CloudRuSTT {
    api_key: String,
    base_url: String,
}

impl CloudRuSTT {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            base_url: "https://foundation-models.api.cloud.ru/v1".to_string(),
        }
    }

    pub async fn transcribe(
        &self,
        samples: &[f32],
        sample_rate: u32,
        language: &str,
    ) -> anyhow::Result<String> {
        let wav_bytes = crate::audio::pcm_to_wav(samples, sample_rate);

        let file_part = multipart::Part::bytes(wav_bytes)
            .file_name("audio.wav")
            .mime_str("audio/wav")?;

        let form = multipart::Form::new()
            .text("model", "whisper-large-v3")
            .text("language", language.to_string())   // "ru" or "en"
            .text("response_format", "json")
            .part("file", file_part);

        let client = reqwest::Client::new();
        let response = client
            .post(format!("{}/audio/transcriptions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .multipart(form)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Cloud.ru API error {status}: {body}"));
        }

        let json: serde_json::Value = response.json().await?;
        json["text"]
            .as_str()
            .map(|s| s.trim().to_string())
            .ok_or_else(|| anyhow::anyhow!("No 'text' field in Cloud.ru response"))
    }
}
```

### Cloud.ru API Key Verification (test call)

```rust
pub async fn verify_api_key(api_key: &str, base_url: &str) -> anyhow::Result<bool> {
    // Use models list endpoint to verify key without uploading audio
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/models", base_url))
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await?;
    Ok(resp.status().is_success())
}
```

---

## OpenRouter — Multimodal Audio (via audio-capable LLMs)

OpenRouter routes to models supporting audio input (e.g. `openai/gpt-4o-audio-preview`).
Note: not all OpenRouter models support audio — check model capabilities.

### openrouter.rs Implementation

```rust
// src-tauri/src/stt/openrouter.rs
use base64::{Engine as _, engine::general_purpose};

pub struct OpenRouterSTT {
    api_key: String,
    model: String,  // e.g. "openai/gpt-4o-audio-preview" or "openai/whisper-1"
}

impl OpenRouterSTT {
    pub fn new(api_key: String, model: String) -> Self {
        Self { api_key, model }
    }

    pub async fn transcribe(
        &self,
        samples: &[f32],
        sample_rate: u32,
        language: &str,
    ) -> anyhow::Result<String> {
        // Option A: whisper-compatible endpoint (if model supports it)
        if self.model.contains("whisper") {
            return self.transcribe_whisper_api(samples, sample_rate, language).await;
        }
        // Option B: multimodal chat with audio attachment
        self.transcribe_via_chat(samples, sample_rate, language).await
    }

    // For whisper-1 via OpenRouter (OpenAI-compatible audio endpoint)
    async fn transcribe_whisper_api(
        &self,
        samples: &[f32],
        sample_rate: u32,
        language: &str,
    ) -> anyhow::Result<String> {
        let wav_bytes = crate::audio::pcm_to_wav(samples, sample_rate);

        let file_part = reqwest::multipart::Part::bytes(wav_bytes)
            .file_name("audio.wav")
            .mime_str("audio/wav")?;

        let form = reqwest::multipart::Form::new()
            .text("model", self.model.clone())
            .text("language", language.to_string())
            .part("file", file_part);

        let client = reqwest::Client::new();
        let response = client
            .post("https://openrouter.ai/api/v1/audio/transcriptions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("HTTP-Referer", "https://github.com/easystt")
            .multipart(form)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("OpenRouter error {status}: {body}"));
        }

        let json: serde_json::Value = response.json().await?;
        Ok(json["text"].as_str().unwrap_or("").trim().to_string())
    }

    // For multimodal models (gpt-4o-audio-preview etc.) — base64 audio in chat
    async fn transcribe_via_chat(
        &self,
        samples: &[f32],
        sample_rate: u32,
        language: &str,
    ) -> anyhow::Result<String> {
        let wav_bytes = crate::audio::pcm_to_wav(samples, sample_rate);
        let b64 = general_purpose::STANDARD.encode(&wav_bytes);

        let lang_hint = match language {
            "ru" => "Transcribe this audio in Russian.",
            "en" => "Transcribe this audio in English.",
            _ => "Transcribe this audio.",
        };

        let body = serde_json::json!({
            "model": self.model,
            "messages": [{
                "role": "user",
                "content": [
                    {
                        "type": "input_audio",
                        "input_audio": {
                            "data": b64,
                            "format": "wav"
                        }
                    },
                    {
                        "type": "text",
                        "text": lang_hint
                    }
                ]
            }]
        });

        let client = reqwest::Client::new();
        let response = client
            .post("https://openrouter.ai/api/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .header("HTTP-Referer", "https://github.com/easystt")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("OpenRouter chat error {status}: {body}"));
        }

        let json: serde_json::Value = response.json().await?;
        Ok(json["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .trim()
            .to_string())
    }
}
```

## Supported Models Reference

| Backend | Model ID | Method | Notes |
|---------|----------|--------|-------|
| Cloud.ru | `whisper-large-v3` | multipart | Best Russian quality |
| OpenRouter | `openai/whisper-1` | multipart | Reliable, fast |
| OpenRouter | `openai/gpt-4o-audio-preview` | base64 chat | High quality, expensive |

## Language Codes

| Language | Code |
|----------|------|
| Russian | `ru` |
| English | `en` |
| Auto-detect | `""` (empty string) |

## Key Rules
- Cloud.ru base URL: `https://foundation-models.api.cloud.ru/v1` — verified from official docs
- API key created in Cloud.ru console: Users → Service Accounts → Create API key (select Foundation Models)
- Always send audio as **WAV 16kHz mono** — most universally accepted format
- For OpenRouter: include `HTTP-Referer` header to avoid 403 errors
- Store API keys in `tauri-plugin-store`, never in `tauri.conf.json` or source code
- Add timeout to all API calls: `.timeout(std::time::Duration::from_secs(60))`
- Cloud.ru docs: https://cloud.ru/docs/foundation-models/ug/topics/quickstart
