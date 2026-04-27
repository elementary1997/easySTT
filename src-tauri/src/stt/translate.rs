/// Real-time translation via any OpenAI-compatible chat completions endpoint.
///
/// Two built-in routes:
///  1. OpenRouter (`openai/gpt-4o-mini`) — when `openrouter_api_key` is set.
///  2. Cloud.ru Foundation Models — when `cloudru_*` credentials are set.
///     Cloud.ru exposes the same `/v1/chat/completions` endpoint, so we reuse
///     the same function with a different URL / Bearer token.
///
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
        _ => "the target language",
    }
}

/// Low-level translation call against any OpenAI-compatible `/v1/chat/completions` endpoint.
pub async fn translate_via_endpoint(
    text: &str,
    from_lang: &str,
    to_lang: &str,
    api_url: &str,
    api_key: &str,
    model: &str,
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
        "model": model,
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
        .post(api_url)
        .header("Authorization", format!("Bearer {api_key}"))
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

/// Translate `text` from `from_lang` to `to_lang` using **OpenRouter** (`openai/gpt-4o-mini`).
pub async fn translate_via_openrouter(
    text: &str,
    from_lang: &str,
    to_lang: &str,
    api_key: &str,
) -> anyhow::Result<String> {
    translate_via_endpoint(
        text,
        from_lang,
        to_lang,
        "https://openrouter.ai/api/v1/chat/completions",
        api_key,
        "openai/gpt-4o-mini",
    )
    .await
}

/// Translate `text` from `from_lang` to `to_lang` using **Cloud.ru** Foundation Models.
///
/// Requires an already-resolved Bearer token (call `bearer_for_stt` first).
/// Uses `Qwen/Qwen2.5-72B-Instruct` — solid multilingual translation quality.
pub async fn translate_via_cloudru(
    text: &str,
    from_lang: &str,
    to_lang: &str,
    bearer_token: &str,
    base_url: &str,
) -> anyhow::Result<String> {
    let url = format!(
        "{}/chat/completions",
        base_url.trim().trim_end_matches('/')
    );
    translate_via_endpoint(
        text,
        from_lang,
        to_lang,
        &url,
        bearer_token,
        "Qwen/Qwen2.5-72B-Instruct",
    )
    .await
}

