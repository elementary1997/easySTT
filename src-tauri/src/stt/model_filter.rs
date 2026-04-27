use serde_json::Value;

/// Эвристика по id: STT / речь → текст (не общий чат).
fn id_suggests_stt(id: &str) -> bool {
    let t = id.to_lowercase();
    t.contains("whisper")
        || t.contains("transcrib")
        || t.contains("speech-to-text")
        || t.contains("speech_to_text")
        || t.contains("/stt")
        || t.contains("stt-")
        || t.contains("asr")
        || t.contains("voice-to-text")
        || (t.contains("gpt-4o") && t.contains("transcribe"))
        || t.contains("deepgram")
        || t.contains("scribe")
}

fn architecture_has_audio_in(model: &Value) -> bool {
    model
        .get("architecture")
        .and_then(|a| a.get("input_modalities"))
        .and_then(|v| v.as_array())
        .is_some_and(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str())
                .any(|m| m == "audio" || m == "input_audio")
        })
}

/// Только кандидаты на распознавание речи / аудио, не весь каталог.
pub fn is_stt_catalog_entry(model: &Value) -> bool {
    let id = model.get("id").and_then(|i| i.as_str()).unwrap_or("");
    if id_suggests_stt(id) {
        return true;
    }
    if architecture_has_audio_in(model) {
        let t = id.to_lowercase();
        return t.contains("whisper")
            || t.contains("transcrib")
            || t.contains("asr")
            || t.contains("speech");
    }
    if let Some(cap) = model.get("capabilities") {
        if let Some(m) = cap.get("stt") {
            if m.as_bool() == Some(true) {
                return true;
            }
        }
        for k in [
            "audio", "asr", "transcription", "transcribe", "whisper",
        ] {
            if cap.get(k).and_then(|c| c.as_bool()) == Some(true) {
                return true;
            }
        }
    }
    false
}
