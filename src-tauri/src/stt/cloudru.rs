use super::SttBackend;
use crate::audio::{pcm_to_wav, resample_to_16k};
use async_trait::async_trait;
use reqwest::multipart;

pub struct CloudRuStt {
    pub api_key: String,
    pub base_url: String,
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

        let file_part = multipart::Part::bytes(wav)
            .file_name("audio.wav")
            .mime_str("audio/wav")?;

        let mut form = multipart::Form::new()
            .text("model", "whisper-large-v3")
            .text("response_format", "json")
            .part("file", file_part);

        if !language.is_empty() && language != "auto" {
            form = form.text("language", language.to_owned());
        }

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()?;

        let response = client
            .post(format!("{}/audio/transcriptions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .multipart(form)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Cloud.ru API {status}: {body}"));
        }

        let json: serde_json::Value = response.json().await?;
        Ok(json["text"]
            .as_str()
            .unwrap_or("")
            .trim()
            .to_string())
    }
}
