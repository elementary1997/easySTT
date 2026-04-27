use async_trait::async_trait;

pub mod cloudru;
pub mod local;
pub mod model_catalog;
pub mod model_filter;
pub mod openrouter;
pub mod translate;

#[async_trait]
pub trait SttBackend: Send + Sync {
    async fn transcribe(
        &self,
        samples: &[f32],
        sample_rate: u32,
        language: &str,
    ) -> anyhow::Result<String>;
}
