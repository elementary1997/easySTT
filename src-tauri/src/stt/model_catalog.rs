use serde::Serialize;

const HF_BASE: &str =
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

/// Описание одной GGML-модели Whisper, доступной для скачивания.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    /// Внутренний ключ (используется в config.local_model_name и model_path()).
    pub id: String,
    /// Название для пользователя.
    pub display_name: String,
    /// Имя файла на диске (ggml-{id}.bin).
    pub filename: String,
    /// Примерный размер в МБ.
    pub size_mb: u64,
    /// URL для скачивания.
    pub url: String,
    /// Краткое описание.
    pub description: String,
    /// Короткий тег для UI (например «INT8», «Turbo»).
    pub tag: Option<String>,
}

impl ModelInfo {
    fn new(
        id: &str,
        display_name: &str,
        size_mb: u64,
        description: &str,
        tag: Option<&str>,
    ) -> Self {
        let filename = format!("ggml-{id}.bin");
        let url = format!("{HF_BASE}/{filename}");
        Self {
            id: id.to_string(),
            display_name: display_name.to_string(),
            filename,
            size_mb,
            url,
            description: description.to_string(),
            tag: tag.map(String::from),
        }
    }
}

/// Каталог доступных для скачивания Whisper-моделей в формате GGML.
/// Все файлы берутся с huggingface.co/ggerganov/whisper.cpp.
pub fn model_catalog() -> Vec<ModelInfo> {
    vec![
        ModelInfo::new(
            "small",
            "Small",
            466,
            "Хорошее качество, умеренная скорость (~466 МБ)",
            None,
        ),
        ModelInfo::new(
            "medium",
            "Medium",
            1500,
            "Высокое качество, медленная без GPU (~1.5 ГБ)",
            None,
        ),
        ModelInfo::new(
            "large-v3",
            "Large V3",
            3094,
            "Лучшее качество, нужна GPU (~3.1 ГБ)",
            None,
        ),
        ModelInfo::new(
            "large-v3-turbo",
            "Large V3 Turbo",
            1623,
            "Дистиллированный V3: быстрее при сопоставимом качестве (~1.6 ГБ)",
            Some("Turbo"),
        ),
    ]
}
