use crate::audio::resample_to_16k;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

/// Cached whisper model to avoid reloading from disk on every PTT.
pub struct LocalWhisperCache {
    pub model_path: String,
    ctx: WhisperContext,
}

/// Оптимальное число потоков для whisper.cpp.
///
/// whisper.cpp практически не ускоряется свыше 8 потоков: его вычислительные ядра
/// ограничены пропускной способностью памяти, и лишние потоки добавляют накладные
/// расходы и конкуренцию за кэш. На 16-поточных машинах `clamp(1, 32)` давало бы
/// 16 потоков — хуже, чем 4–8 физических ядер.
fn n_threads() -> i32 {
    let logical = std::thread::available_parallelism()
        .map(|n| n.get() as i32)
        .unwrap_or(4);
    // Используем не более половины логических ядер (≈ физические ядра),
    // но не больше 8 — это sweet-spot для whisper.cpp на большинстве CPU.
    (logical / 2).clamp(2, 8)
}

/// Trim leading/trailing near-silence to reduce Whisper compute time.
fn trim_silence(samples: &[f32], sample_rate: u32) -> Vec<f32> {
    if samples.is_empty() {
        return Vec::new();
    }
    let sr = sample_rate.max(1) as usize;
    let frame = (sr / 200).max(1); // ~5ms
    let threshold = 0.008f32;
    let keep_pad = sr / 5; // 200ms pad around speech

    let mut first = None;
    for i in (0..samples.len()).step_by(frame) {
        let end = (i + frame).min(samples.len());
        let peak = samples[i..end]
            .iter()
            .map(|v| v.abs())
            .fold(0.0f32, f32::max);
        if peak >= threshold {
            first = Some(i);
            break;
        }
    }

    let mut last = None;
    let mut i = samples.len().saturating_sub(frame);
    loop {
        let end = (i + frame).min(samples.len());
        let peak = samples[i..end]
            .iter()
            .map(|v| v.abs())
            .fold(0.0f32, f32::max);
        if peak >= threshold {
            last = Some(end);
            break;
        }
        if i == 0 {
            break;
        }
        i = i.saturating_sub(frame);
    }

    match (first, last) {
        (Some(a), Some(b)) if b > a => {
            let s = a.saturating_sub(keep_pad);
            let e = (b + keep_pad).min(samples.len());
            samples[s..e].to_vec()
        }
        _ => samples.to_vec(),
    }
}

/// Параметры инициализации модели. При сборке с `whisper-cuda` / `vulkan` / `metal` в [WhisperContextParameters]
/// по умолчанию включён GPU. Переменная `EASYSTT_WHISPER_NO_GPU=1` принудительно отключает GPU.
fn whisper_init_parameters(use_gpu: bool) -> WhisperContextParameters<'static> {
    let mut p = WhisperContextParameters::default();
    p.use_gpu(use_gpu);
    if std::env::var("EASYSTT_WHISPER_NO_GPU").as_deref() == Ok("1") {
        p.use_gpu(false);
    }
    p
}

/// Run recognition reusing a loaded [WhisperContext] when the model path matches.
pub async fn transcribe_cached(
    cache: Arc<Mutex<Option<LocalWhisperCache>>>,
    model_path: String,
    samples: Vec<f32>,
    sample_rate: u32,
    language: &str,
    cancel: Arc<AtomicBool>,
    use_gpu: bool,
) -> anyhow::Result<String> {
    let resampled = resample_to_16k(&samples, sample_rate);
    // GPU-режим: не обрезаем тишину — Vulkan-бэкенд требует минимум ~1 сек аудио
    // и плохо работает с очень короткими буферами; скорость GPU и без того высокая.
    // CPU-режим: обрезаем для ускорения инференса.
    let trimmed = if use_gpu {
        resampled.clone()
    } else {
        trim_silence(&resampled, 16_000)
    };
    let lang = if language.is_empty() || language == "auto" {
        None
    } else {
        Some(language.to_owned())
    };

    let model_path_m = model_path.clone();
    let text = tokio::task::spawn_blocking(move || -> anyhow::Result<String> {
        if cancel.load(Ordering::SeqCst) {
            return Err(anyhow::anyhow!("Обработка отменена"));
        }
        let mut g = cache.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
        let need = g
            .as_ref()
            .map(|c| c.model_path != model_path_m)
            .unwrap_or(true);
        if need {
            let ctx = WhisperContext::new_with_params(
                &model_path_m,
                whisper_init_parameters(use_gpu),
            )
            .map_err(|e| anyhow::anyhow!("Не удалось загрузить модель: {e}"))?;
            *g = Some(LocalWhisperCache {
                model_path: model_path_m,
                ctx,
            });
        }

        let w = g.as_mut().ok_or_else(|| anyhow::anyhow!("Кэш модели пуст"))?;
        let ctx = &w.ctx;

        let mut state = ctx
            .create_state()
            .map_err(|e| anyhow::anyhow!("Ошибка создания состояния: {e}"))?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        // GPU-режим: n_threads=1 — тяжёлая работа делается на GPU, CPU-потоки
        // только мешают и могут конфликтовать с Vulkan-шейдерами.
        // CPU-режим: оптимальное число потоков из n_threads().
        params.set_n_threads(if use_gpu { 1 } else { n_threads() });
        params.set_language(lang.as_deref());
        // Faster one-shot dictation profile
        params.set_no_context(true);
        params.set_single_segment(true);
        params.set_no_timestamps(true);
        // audio_ctx: ограничиваем контекст кодировщика для ускорения на CPU.
        // На GPU (Vulkan/CUDA) — всегда полный контекст (0 = 1500 фреймов):
        // уменьшенный audio_ctx вызывает несоответствие размеров тензоров в GPU-бэкенде.
        // EASYSTT_WHISPER_FULL_AUDIO_CTX=1 — принудительно полный контекст в любом режиме.
        let force_full_ctx = use_gpu
            || std::env::var("EASYSTT_WHISPER_FULL_AUDIO_CTX").as_deref() == Ok("1");
        if force_full_ctx {
            params.set_audio_ctx(0);
        } else {
            let sec = (trimmed.len() as f32) / 16_000.0f32;
            let ctx = if sec <= 0.0 {
                0
            } else if sec <= 6.0 {
                256 // ≤6 сек: 256 фреймов — для коротких PTT-фраз на CPU
            } else if sec <= 20.0 {
                512 // 6–20 сек: 512 фреймов на CPU
            } else {
                0   // >20 сек: полный контекст
            };
            params.set_audio_ctx(ctx);
        }
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_suppress_blank(true);
        // GPU-режим: abort-коллбэк отключён — Rust-замыкание через FFI может
        // нарушить синхронизацию Vulkan-потоков и вызвать error -6.
        // CPU-режим: коллбэк используется для быстрой отмены.
        if !use_gpu {
            let c = cancel.clone();
            params.set_abort_callback_safe(move || c.load(Ordering::SeqCst));
        }

        state
            .full(params, &trimmed)
            .map_err(|e| anyhow::anyhow!("Ошибка распознавания: {e}"))?;

        if cancel.load(Ordering::SeqCst) {
            return Err(anyhow::anyhow!("Обработка отменена"));
        }

        let n = state
            .full_n_segments()
            .map_err(|e| anyhow::anyhow!("Ошибка получения сегментов: {e}"))?;

        let text = (0..n)
            .filter_map(|i| state.full_get_segment_text(i).ok())
            .collect::<Vec<_>>()
            .join(" ");

        Ok(text.trim().to_string())
    })
    .await??;

    Ok(text)
}
