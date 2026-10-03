use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

#[cfg(target_os = "linux")]
#[path = "audio/linux.rs"]
mod linux;

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Microphone {
    pub id: String,
    pub name: String,
    pub is_default: bool,
    pub aliases: Vec<String>,
}
use std::sync::{Arc, Mutex};

pub struct AudioRecorder {
    samples: Arc<Mutex<Vec<f32>>>,
    sample_rate: Arc<Mutex<u32>>,
    stream: Option<cpal::Stream>,
    #[cfg(target_os = "linux")]
    pulse_capture: Option<linux::Capture>,
}

// cpal::Stream is !Send, but we keep it behind Option and only access it from
// the thread that creates/drops it — safe to mark Send for Tauri state.
unsafe impl Send for AudioRecorder {}

impl AudioRecorder {
    pub fn new() -> Self {
        Self {
            samples: Arc::new(Mutex::new(Vec::new())),
            sample_rate: Arc::new(Mutex::new(44100)),
            stream: None,
            #[cfg(target_os = "linux")]
            pulse_capture: None,
        }
    }

    pub fn start(&mut self, device_name: &str) -> anyhow::Result<()> {
        if self.is_active() {
            anyhow::bail!("Запись уже идёт");
        }
        self.samples.lock().unwrap().clear();
        #[cfg(target_os = "linux")]
        {
            let devices = if device_name.starts_with("pulse:") {
                None
            } else {
                linux::microphones().ok()
            };
            if device_name.is_empty() {
                if let Some(devices) = &devices {
                    if !devices.iter().any(|d| d.is_default) {
                        anyhow::bail!(
                            "Системный вход не является микрофоном. Выберите микрофон в настройках"
                        );
                    }
                }
            }
            let source = device_name
                .strip_prefix("pulse:")
                .map(String::from)
                .or_else(|| {
                    devices
                        .as_ref()?
                        .iter()
                        .find(|d| d.aliases.iter().any(|id| id == device_name))
                        .map(|d| d.id.trim_start_matches("pulse:").to_string())
                });
            if source.is_some() || (device_name.is_empty() && devices.is_some()) {
                // With a working server, errors must not silently switch to a
                // different hardware/default input through ALSA.
                let capture = linux::Capture::start(source, Arc::clone(&self.samples))?;
                *self.sample_rate.lock().unwrap() = linux::SAMPLE_RATE;
                self.pulse_capture = Some(capture);
                return Ok(());
            }
        }
        let host = cpal::default_host();
        let device = if device_name.is_empty() {
            host.default_input_device()
                .ok_or_else(|| anyhow::anyhow!("Нет устройства ввода"))?
        } else {
            host.input_devices()?
                .find(|d| d.name().ok().as_deref() == Some(device_name))
                .ok_or_else(|| anyhow::anyhow!("Микрофон не найден: {device_name}"))?
        };

        let supported = device.default_input_config()?;
        let rate = supported.sample_rate().0;
        *self.sample_rate.lock().unwrap() = rate;

        let samples = Arc::clone(&self.samples);
        samples.lock().unwrap().clear();

        let config: cpal::StreamConfig = supported.clone().into();

        let stream = match supported.sample_format() {
            cpal::SampleFormat::I8 => build_stream::<i8>(&device, &config, samples),
            cpal::SampleFormat::I16 => build_stream::<i16>(&device, &config, samples),
            cpal::SampleFormat::I32 => build_stream::<i32>(&device, &config, samples),
            cpal::SampleFormat::I64 => build_stream::<i64>(&device, &config, samples),
            cpal::SampleFormat::U8 => build_stream::<u8>(&device, &config, samples),
            cpal::SampleFormat::U16 => build_stream::<u16>(&device, &config, samples),
            cpal::SampleFormat::U32 => build_stream::<u32>(&device, &config, samples),
            cpal::SampleFormat::U64 => build_stream::<u64>(&device, &config, samples),
            cpal::SampleFormat::F32 => build_stream::<f32>(&device, &config, samples),
            cpal::SampleFormat::F64 => build_stream::<f64>(&device, &config, samples),
            format => anyhow::bail!("Неподдерживаемый формат микрофона: {format:?}"),
        }?;

        stream.play()?;
        self.stream = Some(stream);
        Ok(())
    }

    pub fn stop(&mut self) -> (Vec<f32>, u32) {
        #[cfg(target_os = "linux")]
        self.pulse_capture.take(); // stop and join before reading the final buffer
        self.stream.take(); // drop stops capture
        let samples = self.samples.lock().unwrap().clone();
        let rate = *self.sample_rate.lock().unwrap();
        (samples, rate)
    }

    /// Возвращает Arc на буфер сэмплов — для фонового always-on задания.
    pub fn samples_arc(&self) -> Arc<Mutex<Vec<f32>>> {
        Arc::clone(&self.samples)
    }

    pub fn sample_rate_arc(&self) -> Arc<Mutex<u32>> {
        Arc::clone(&self.sample_rate)
    }

    pub fn is_active(&self) -> bool {
        #[cfg(target_os = "linux")]
        if self
            .pulse_capture
            .as_ref()
            .map(|c| c.is_active())
            .unwrap_or(false)
        {
            return true;
        }
        self.stream.is_some()
    }
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    samples: Arc<Mutex<Vec<f32>>>,
) -> Result<cpal::Stream, cpal::BuildStreamError>
where
    T: cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    let channels = config.channels as usize;
    device.build_input_stream(
        config,
        move |data: &[T], _| {
            samples
                .lock()
                .unwrap()
                .extend(data.chunks_exact(channels).map(|frame| {
                    frame
                        .iter()
                        .map(|&sample| cpal::Sample::to_sample::<f32>(sample))
                        .sum::<f32>()
                        / channels as f32
                }));
        },
        |error| eprintln!("audio error: {error}"),
        None,
    )
}

pub fn list_microphones() -> Vec<Microphone> {
    #[cfg(target_os = "linux")]
    if let Ok(devices) = linux::microphones() {
        return devices;
    }
    let host = cpal::default_host();
    let default_name = host.default_input_device().and_then(|d| d.name().ok());
    let mut devices: Vec<Microphone> = host
        .input_devices()
        .map(|devices| {
            devices
                .filter_map(|device| {
                    let id = device.name().ok()?;
                    #[cfg(target_os = "linux")]
                    let name = {
                        // Without a desktop sound server, keep one convertible PCM per
                        // hardware input instead of exposing ALSA's many service aliases.
                        let card = id.strip_prefix("plughw:CARD=")?.split(',').next()?;
                        let cards = std::fs::read_to_string("/proc/asound/cards").ok()?;
                        let line = cards.lines().find(|line| {
                            line.split_once('[')
                                .and_then(|(_, rest)| rest.split_once(']'))
                                .map(|(value, _)| value.trim() == card)
                                .unwrap_or(false)
                        })?;
                        let label = line.split_once(" - ")?.1.trim();
                        let dev = id.split("DEV=").nth(1).unwrap_or("0");
                        if dev == "0" {
                            label.to_string()
                        } else {
                            format!("{label} (вход {dev})")
                        }
                    };
                    #[cfg(not(target_os = "linux"))]
                    let name = id.clone();
                    Some(Microphone {
                        is_default: default_name.as_ref() == Some(&id),
                        id,
                        name,
                        aliases: Vec::new(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    devices.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    devices.dedup_by(|a, b| a.id == b.id);
    devices
}

/// Resample mono f32 audio from `from_rate` to 16000 Hz (required by Whisper).
pub fn resample_to_16k(samples: &[f32], from_rate: u32) -> Vec<f32> {
    if from_rate == 16000 {
        return samples.to_vec();
    }
    let ratio = from_rate as f64 / 16000.0;
    let out_len = (samples.len() as f64 / ratio) as usize;
    (0..out_len)
        .map(|i| {
            let src = i as f64 * ratio;
            let idx = src as usize;
            let frac = (src - idx as f64) as f32;
            let a = samples.get(idx).copied().unwrap_or(0.0);
            let b = samples.get(idx + 1).copied().unwrap_or(0.0);
            a + (b - a) * frac
        })
        .collect()
}

/// Encode mono f32 PCM to WAV bytes (16-bit, little-endian).
pub fn pcm_to_wav(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    let num_samples = samples.len() as u32;
    let data_size = num_samples * 2;
    let file_size = 36 + data_size;

    let mut wav = Vec::with_capacity((file_size + 8) as usize);

    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&file_size.to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&1u16.to_le_bytes()); // mono
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&(sample_rate * 2).to_le_bytes()); // byte rate
    wav.extend_from_slice(&2u16.to_le_bytes()); // block align
    wav.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());

    for &s in samples {
        let pcm = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
        wav.extend_from_slice(&pcm.to_le_bytes());
    }
    wav
}
