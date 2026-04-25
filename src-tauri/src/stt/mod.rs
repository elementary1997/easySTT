use async_trait::async_trait;

pub mod cloudru;
pub mod local;
pub mod openrouter;

#[async_trait]
pub trait SttBackend: Send + Sync {
    async fn transcribe(
        &self,
        samples: &[f32],
        sample_rate: u32,
        language: &str,
    ) -> anyhow::Result<String>;
}
