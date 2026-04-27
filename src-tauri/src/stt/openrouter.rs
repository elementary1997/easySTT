use super::SttBackend;
use crate::audio::{pcm_to_wav, resample_to_16k};
use async_trait::async_trait;
use base64::{engine::general_purpose, Engine as _};
use reqwest::multipart;

pub struct OpenRouterStt {
    pub api_key: String,
    pub model: String,
}

#[async_trait]
impl SttBackend for OpenRouterStt {
    async fn transcribe(
        &self,
        samples: &[f32],
        sample_rate: u32,
        language: &str,
    ) -> anyhow::Result<String> {
        let resampled = resample_to_16k(samples, sample_rate);
        let wav = pcm_to_wav(&resampled, 16000);

        // whisper-1 and similar models: use multipart audio endpoint
        if self.model.contains("whisper") {
            return self.transcribe_multipart(wav, language).await;
        }
        // Multimodal LLMs: base64 audio in chat message
        self.transcribe_chat(wav, language).await
    }
}

impl OpenRouterStt {
    async fn post_audio_endpoint(
        &self,
        endpoint: &str, // "transcriptions" | "translations"
        wav: Vec<u8>,
        language: &str, // ignored for translations
    ) -> anyhow::Result<String> {
        let file_part = multipart::Part::bytes(wav)
            .file_name("audio.wav")
            .mime_str("audio/wav")?;

        let mut form = multipart::Form::new()
            .text("model", self.model.clone())
            .part("file", file_part);

        if endpoint == "transcriptions" && !language.is_empty() && language != "auto" {
            form = form.text("language", language.to_owned());
        }

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()?;

        let url = format!("https://openrouter.ai/api/v1/audio/{endpoint}");
        let response = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("HTTP-Referer", "https://github.com/easystt/easystt")
            .multipart(form)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("OpenRouter {status}: {body}"));
        }

        let json: serde_json::Value = response.json().await?;
        Ok(json["text"].as_str().unwrap_or("").trim().to_string())
    }

    async fn transcribe_multipart(&self, wav: Vec<u8>, language: &str) -> anyhow::Result<String> {
        self.post_audio_endpoint("transcriptions", wav, language).await
    }

    /// Use Whisper's built-in translate task via `/audio/translations` (audio → English text).
    pub async fn translate_audio(&self, samples: &[f32], sample_rate: u32) -> anyhow::Result<String> {
        use crate::audio::{pcm_to_wav, resample_to_16k};
        let resampled = resample_to_16k(samples, sample_rate);
        let wav = pcm_to_wav(&resampled, 16000);
        self.post_audio_endpoint("translations", wav, "").await
    }

    async fn transcribe_chat(&self, wav: Vec<u8>, language: &str) -> anyhow::Result<String> {
        let b64 = general_purpose::STANDARD.encode(&wav);

        let lang_hint = match language {
            "ru" => "Transcribe this audio verbatim in Russian. Output only the transcription.",
            "en" => "Transcribe this audio verbatim in English. Output only the transcription.",
            _ => "Transcribe this audio verbatim. Output only the transcription.",
        };

        let body = serde_json::json!({
            "model": self.model,
            "messages": [{
                "role": "user",
                "content": [
                    {
                        "type": "input_audio",
                        "input_audio": { "data": b64, "format": "wav" }
                    },
                    { "type": "text", "text": lang_hint }
                ]
            }]
        });

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()?;

        let response = client
            .post("https://openrouter.ai/api/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("HTTP-Referer", "https://github.com/easystt/easystt")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("OpenRouter chat {status}: {body}"));
        }

        let json: serde_json::Value = response.json().await?;
        Ok(json["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .trim()
            .to_string())
    }
}
