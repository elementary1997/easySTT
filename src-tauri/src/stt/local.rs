use super::SttBackend;
use crate::audio::resample_to_16k;
use async_trait::async_trait;

/// Stub implementation — real whisper-rs integration is behind the `local-stt` feature flag.
/// To enable: uncomment whisper-rs in Cargo.toml and replace this file.
pub struct LocalStt {
    pub model_path: String,
}

#[async_trait]
impl SttBackend for LocalStt {
    async fn transcribe(
        &self,
        samples: &[f32],
        sample_rate: u32,
        _language: &str,
    ) -> anyhow::Result<String> {
        let _resampled = resample_to_16k(samples, sample_rate);

        // TODO: replace with whisper-rs when feature is enabled
        // use whisper_rs::{WhisperContext, WhisperContextParameters, FullParams, SamplingStrategy};
        // let ctx = WhisperContext::new_with_params(&self.model_path, WhisperContextParameters::default())?;
        // let mut state = ctx.create_state()?;
        // let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        // params.set_language(Some(_language));
        // params.set_print_progress(false);
        // state.full(params, &_resampled)?;
        // let n = state.full_n_segments()?;
        // let text = (0..n).map(|i| state.full_get_segment_text(i).unwrap_or_default()).collect::<Vec<_>>().join(" ");
        // return Ok(text.trim().to_string());

        Err(anyhow::anyhow!(
            "Локальная модель не активирована. Включите feature 'local-stt' и укажите путь к модели: {}",
            self.model_path
        ))
    }
}
