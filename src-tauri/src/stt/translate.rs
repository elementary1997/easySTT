/// Real-time translation via OpenRouter chat completions.
///
/// Uses `openai/gpt-4o-mini` — fast, cheap, excellent translation quality.
/// Called AFTER transcription when `Config::translate_enabled == true` and
/// the translation cannot be handled natively by Whisper (i.e. target ≠ English
/// or backend ≠ local).
use serde_json::json;

/// Human-readable name for a language code (used in the system prompt).
pub fn lang_name(code: &str) -> &'static str {
    match code {
        "en" => "English",
        "ru" => "Russian",
        "de" => "German",
        "fr" => "French",
        "es" => "Spanish",
        "zh" => "Chinese",
        "ja" => "Japanese",
        "ko" => "Korean",
        "ar" => "Arabic",
        "it" => "Italian",
        "pt" => "Portuguese",
        "nl" => "Dutch",
        "pl" => "Polish",
        "tr" => "Turkish",
        "uk" => "Ukrainian",
        other => {
            // Return a static reference to a known string or fall back to "the target language"
            // For unknown codes, we can't return a reference to the input `other` directly
            // (lifetime issue), so fall back gracefully.
            let _ = other;
            "the target language"
        }
    }
}

/// Translate `text` from `from_lang` to `to_lang` using OpenRouter chat API.
///
/// `from_lang` may be `"auto"` — in that case the model infers the source language.
pub async fn translate_via_llm(
    text: &str,
    from_lang: &str,
    to_lang: &str,
    openrouter_key: &str,
) -> anyhow::Result<String> {
    if text.trim().is_empty() {
        return Ok(String::new());
    }

    let to_label = lang_name(to_lang);
    let from_clause = if from_lang == "auto" {
        "Detect the source language automatically.".to_string()
    } else {
        format!("Source language: {}.", lang_name(from_lang))
    };

    let system = format!(
        "You are a professional real-time translator. {from_clause} \
         Translate the following text to {to_label}. \
         Output ONLY the translated text — no explanations, no quotes, no notes.",
    );

    let body = json!({
        "model": "openai/gpt-4o-mini",
        "messages": [
            {"role": "system", "content": system},
            {"role": "user",   "content": text}
        ],
        "max_tokens": 2000,
        "temperature": 0.1
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let response = client
        .post("https://openrouter.ai/api/v1/chat/completions")
        .header("Authorization", format!("Bearer {openrouter_key}"))
        .header("HTTP-Referer", "https://github.com/easystt/easystt")
        .json(&body)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("Translation API {status}: {body}"));
    }

    let json: serde_json::Value = response.json().await?;
    Ok(json["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .trim()
        .to_string())
}
