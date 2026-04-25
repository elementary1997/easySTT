---
name: audio-stt
description: Audio capture and STT integration for easySTT — cpal recording, push-to-talk buffer management, audio format conversion to WAV/PCM, whisper-rs local inference. Use when working on audio.rs, local STT backend, or preparing audio for API calls.
version: 1.0.0
---

# Audio Capture and STT Integration

## Audio Pipeline Overview

```
Microphone → cpal stream → ring buffer → [PTT gating] → Vec<f32> PCM
    → [Local] whisper-rs → String
    → [Cloud]  f32 PCM → WAV bytes → multipart HTTP → String
```

## cpal: Basic Audio Capture

```rust
// src-tauri/src/audio.rs
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};

pub struct AudioRecorder {
    samples: Arc<Mutex<Vec<f32>>>,
    stream: Option<cpal::Stream>,
}

impl AudioRecorder {
    pub fn new() -> Self {
        Self { samples: Arc::new(Mutex::new(Vec::new())), stream: None }
    }

    pub fn start(&mut self) -> anyhow::Result<()> {
        let host = cpal::default_host();
        let device = host.default_input_device()
            .ok_or_else(|| anyhow::anyhow!("No input device"))?;

        let config = device.default_input_config()?;
        let samples = Arc::clone(&self.samples);
        samples.lock().unwrap().clear();

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => build_stream::<f32>(&device, &config.into(), samples)?,
            cpal::SampleFormat::I16 => build_stream::<i16>(&device, &config.into(), samples)?,
            cpal::SampleFormat::U16 => build_stream::<u16>(&device, &config.into(), samples)?,
            _ => return Err(anyhow::anyhow!("Unsupported sample format")),
        };

        stream.play()?;
        self.stream = Some(stream);
        Ok(())
    }

    pub fn stop(&mut self) -> Vec<f32> {
        self.stream.take(); // drops stream, stops capture
        self.samples.lock().unwrap().clone()
    }
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    samples: Arc<Mutex<Vec<f32>>>,
) -> anyhow::Result<cpal::Stream>
where
    T: cpal::Sample + cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    let err_fn = |e| eprintln!("audio error: {e}");
    let stream = device.build_input_stream(
        config,
        move |data: &[T], _| {
            let mut buf = samples.lock().unwrap();
            buf.extend(data.iter().map(|s| f32::from_sample(*s)));
        },
        err_fn,
        None,
    )?;
    Ok(stream)
}
```

## Converting f32 PCM to WAV bytes (for API upload)

```rust
pub fn pcm_to_wav(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    let num_samples = samples.len() as u32;
    let data_size = num_samples * 2; // 16-bit PCM
    let file_size = 36 + data_size;

    let mut wav = Vec::with_capacity((file_size + 8) as usize);

    // RIFF header
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&file_size.to_le_bytes());
    wav.extend_from_slice(b"WAVE");

    // fmt chunk
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());  // chunk size
    wav.extend_from_slice(&1u16.to_le_bytes());   // PCM
    wav.extend_from_slice(&1u16.to_le_bytes());   // mono
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&(sample_rate * 2).to_le_bytes()); // byte rate
    wav.extend_from_slice(&2u16.to_le_bytes());   // block align
    wav.extend_from_slice(&16u16.to_le_bytes());  // bits per sample

    // data chunk
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());
    for &s in samples {
        let pcm = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
        wav.extend_from_slice(&pcm.to_le_bytes());
    }
    wav
}
```

## Resampling to 16kHz (required by Whisper)

```rust
// Whisper requires 16kHz mono f32
pub fn resample_to_16k(samples: &[f32], from_rate: u32) -> Vec<f32> {
    if from_rate == 16000 {
        return samples.to_vec();
    }
    let ratio = from_rate as f64 / 16000.0;
    let out_len = (samples.len() as f64 / ratio) as usize;
    (0..out_len).map(|i| {
        let src_pos = i as f64 * ratio;
        let src_idx = src_pos as usize;
        let frac = (src_pos - src_idx as f64) as f32;
        let a = samples.get(src_idx).copied().unwrap_or(0.0);
        let b = samples.get(src_idx + 1).copied().unwrap_or(0.0);
        a + (b - a) * frac
    }).collect()
}
```

## whisper-rs Local Inference

```toml
# Cargo.toml
[dependencies]
whisper-rs = { version = "0.12", features = ["cuda"] }  # remove cuda for CPU-only
```

```rust
// src-tauri/src/stt/local.rs
use whisper_rs::{WhisperContext, WhisperContextParameters, FullParams, SamplingStrategy};

pub struct LocalSTT {
    ctx: WhisperContext,
}

impl LocalSTT {
    pub fn load(model_path: &str) -> anyhow::Result<Self> {
        let ctx = WhisperContext::new_with_params(
            model_path,
            WhisperContextParameters::default(),
        )?;
        Ok(Self { ctx })
    }

    pub fn transcribe(&self, samples: &[f32], language: &str) -> anyhow::Result<String> {
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some(language));   // "ru" or "en"
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);

        let mut state = self.ctx.create_state()?;
        state.full(params, samples)?;

        let num_segments = state.full_n_segments()?;
        let mut result = String::new();
        for i in 0..num_segments {
            result.push_str(state.full_get_segment_text(i)?.trim());
            result.push(' ');
        }
        Ok(result.trim().to_string())
    }
}
```

## Model Management (separate download, not bundled)

```rust
// src-tauri/src/config.rs
use std::path::PathBuf;

pub fn models_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("easySTT")
        .join("models")
}

pub fn model_path(name: &str) -> PathBuf {
    models_dir().join(format!("{}.bin", name))
}

// Available local models (whisper.cpp format .bin)
pub const LOCAL_MODELS: &[(&str, &str)] = &[
    ("tiny",   "~75 MB, fastest, lower quality"),
    ("base",   "~142 MB, fast, decent quality"),
    ("small",  "~466 MB, good quality"),
    ("medium", "~1.5 GB, great quality"),
    ("large-v3", "~3 GB, best quality, needs GPU"),
];
```

## STT Backend Trait

```rust
// src-tauri/src/stt/mod.rs
use async_trait::async_trait;

#[async_trait]
pub trait STTBackend: Send + Sync {
    async fn transcribe(&self, samples: &[f32], sample_rate: u32, language: &str)
        -> anyhow::Result<String>;
}

pub enum Backend {
    Local(LocalSTT),
    CloudRu(CloudRuSTT),
    OpenRouter(OpenRouterSTT),
}

#[async_trait]
impl STTBackend for Backend {
    async fn transcribe(&self, samples: &[f32], sample_rate: u32, language: &str)
        -> anyhow::Result<String>
    {
        match self {
            Backend::Local(b) => {
                let resampled = resample_to_16k(samples, sample_rate);
                b.transcribe(&resampled, language)
            }
            Backend::CloudRu(b) => b.transcribe(samples, sample_rate, language).await,
            Backend::OpenRouter(b) => b.transcribe(samples, sample_rate, language).await,
        }
    }
}
```

## Key Rules
- Whisper always needs **16kHz mono f32** — always resample before passing to whisper-rs
- For API backends: convert to WAV bytes, send as `multipart/form-data` with field name `file`
- Keep `AudioRecorder` behind a `Mutex` in Tauri state; `stream` is `!Send` so wrap carefully
- Default audio capture is stereo at 44.1/48kHz — always convert to mono 16kHz for Whisper
- Model files go in OS data dir (`%APPDATA%/easySTT/models` on Windows, `~/.local/share/easySTT/models` on Linux)
