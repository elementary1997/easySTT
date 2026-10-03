//! Use the desktop sound server for named, shared microphone capture on Linux.
//! PipeWire exposes the same PulseAudio protocol, including Bluetooth devices.
use super::Microphone;
use libpulse_binding as pulse;
use pulse::callbacks::ListResult;
use pulse::context::{Context, FlagSet as ContextFlags, State as ContextState};
use pulse::mainloop::standard::{IterateResult, Mainloop};
use pulse::stream::{FlagSet as StreamFlags, PeekResult, State as StreamState, Stream};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
pub const SAMPLE_RATE: u32 = 48000;

fn iterate(mainloop: &mut Mainloop) -> anyhow::Result<()> {
    match mainloop.iterate(false) {
        IterateResult::Success(_) => Ok(()),
        _ => anyhow::bail!("Соединение с аудиосервером потеряно"),
    }
}

fn connect() -> anyhow::Result<(Mainloop, Context)> {
    let mut mainloop = Mainloop::new().ok_or_else(|| anyhow::anyhow!("Нет аудиосервера"))?;
    let mut context = Context::new(&mainloop, "easySTT")
        .ok_or_else(|| anyhow::anyhow!("Не удалось создать аудиоконтекст"))?;
    context.connect(None, ContextFlags::NOAUTOSPAWN, None)?;
    let deadline = Instant::now() + CONNECT_TIMEOUT;
    loop {
        iterate(&mut mainloop)?;
        match context.get_state() {
            ContextState::Ready => return Ok((mainloop, context)),
            ContextState::Failed | ContextState::Terminated => {
                anyhow::bail!("PipeWire/PulseAudio недоступен: {}", context.errno());
            }
            _ if Instant::now() >= deadline => anyhow::bail!("Аудиосервер не отвечает"),
            _ => thread::sleep(Duration::from_millis(5)),
        }
    }
}

pub fn microphones() -> anyhow::Result<Vec<Microphone>> {
    let (mut mainloop, mut context) = connect()?;
    let devices = Rc::new(RefCell::new(Vec::new()));
    let default_source = Rc::new(RefCell::new(None));
    let devices_cb = devices.clone();
    let default_cb = default_source.clone();
    let mut server_op = context.introspect().get_server_info(move |info| {
        *default_cb.borrow_mut() = info.default_source_name.as_ref().map(|s| s.to_string());
    });
    let failed = Rc::new(RefCell::new(false));
    let failed_cb = failed.clone();
    let mut sources_op = context.introspect().get_source_info_list(move |result| {
        match result {
            ListResult::Item(info) => {
                // Sink monitors capture speaker output, never microphone input.
                if info.monitor_of_sink.is_some() {
                    return;
                }
                let Some(source) = info.name.as_ref() else {
                    return;
                };
                let name = info
                    .description
                    .as_ref()
                    .map(|s| s.to_string())
                    .or_else(|| info.proplist.get_str("device.description"))
                    .unwrap_or_else(|| "Микрофон".into());
                let mut aliases = Vec::new();
                if let Some(card) = info.proplist.get_str("alsa.card") {
                    if let Ok(id) = std::fs::read_to_string(format!("/proc/asound/card{card}/id")) {
                        let device = info
                            .proplist
                            .get_str("alsa.device")
                            .unwrap_or_else(|| "0".into());
                        for prefix in ["hw", "plughw", "front", "dsnoop"] {
                            aliases.push(format!("{prefix}:CARD={},DEV={device}", id.trim()));
                        }
                        aliases.push(format!("sysdefault:CARD={}", id.trim()));
                    }
                }
                devices_cb.borrow_mut().push(Microphone {
                    id: format!("pulse:{source}"),
                    name,
                    is_default: false,
                    aliases,
                });
            }
            ListResult::Error => *failed_cb.borrow_mut() = true,
            ListResult::End => {}
        }
    });
    let deadline = Instant::now() + CONNECT_TIMEOUT;
    while sources_op.get_state() == pulse::operation::State::Running
        || server_op.get_state() == pulse::operation::State::Running
    {
        iterate(&mut mainloop)?;
        if Instant::now() >= deadline
            || matches!(
                context.get_state(),
                ContextState::Failed | ContextState::Terminated
            )
        {
            sources_op.cancel();
            server_op.cancel();
            anyhow::bail!("Не удалось получить микрофоны от аудиосервера");
        }
        thread::sleep(Duration::from_millis(5));
    }
    if *failed.borrow() {
        anyhow::bail!("Аудиосервер не вернул список микрофонов");
    }
    let mut result = devices.borrow().clone();
    for device in &mut result {
        device.is_default = default_source
            .borrow()
            .as_ref()
            .map(|name| device.id == format!("pulse:{name}"))
            .unwrap_or(false);
    }
    result.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    result.dedup_by(|a, b| a.id == b.id);
    context.disconnect();
    Ok(result)
}

pub struct Capture {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Capture {
    pub fn start(source: Option<String>, samples: Arc<Mutex<Vec<f32>>>) -> anyhow::Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            let setup = (|| -> anyhow::Result<_> {
                let (mut mainloop, mut context) = connect()?;
                let spec = pulse::sample::Spec {
                    format: pulse::sample::Format::FLOAT32NE,
                    channels: 1,
                    rate: SAMPLE_RATE,
                };
                let mut stream = Stream::new(&mut context, "Микрофон", &spec, None)
                    .ok_or_else(|| anyhow::anyhow!("Не удалось открыть микрофон"))?;
                let attr = pulse::def::BufferAttr {
                    maxlength: u32::MAX,
                    tlength: u32::MAX,
                    prebuf: u32::MAX,
                    minreq: u32::MAX,
                    fragsize: SAMPLE_RATE / 50 * 4,
                };
                // Keep an explicitly selected device fixed; default follows desktop routing.
                let flags = if source.is_some() {
                    StreamFlags::DONT_MOVE | StreamFlags::ADJUST_LATENCY
                } else {
                    StreamFlags::ADJUST_LATENCY
                };
                stream.connect_record(source.as_deref(), Some(&attr), flags)?;
                let deadline = Instant::now() + CONNECT_TIMEOUT;
                loop {
                    iterate(&mut mainloop)?;
                    match stream.get_state() {
                        StreamState::Ready => break,
                        StreamState::Failed | StreamState::Terminated => {
                            anyhow::bail!("Микрофон недоступен: {}", context.errno());
                        }
                        _ if Instant::now() >= deadline => anyhow::bail!("Микрофон не отвечает"),
                        _ => thread::sleep(Duration::from_millis(5)),
                    }
                }
                Ok((mainloop, context, stream))
            })();
            let (mut mainloop, mut context, mut stream) = match setup {
                Ok(value) => value,
                Err(error) => {
                    let _ = ready_tx.send(Err(error.to_string()));
                    return;
                }
            };
            if ready_tx.send(Ok(())).is_err() {
                return;
            }
            while !stopped.load(Ordering::Relaxed) {
                if iterate(&mut mainloop).is_err() || stream.get_state() != StreamState::Ready {
                    eprintln!("audio error: microphone disconnected");
                    break;
                }
                loop {
                    match stream.peek() {
                        Ok(PeekResult::Empty) => break,
                        Ok(PeekResult::Data(bytes)) => {
                            samples.lock().unwrap().extend(
                                bytes
                                    .chunks_exact(4)
                                    .map(|b| f32::from_ne_bytes(b.try_into().unwrap())),
                            );
                        }
                        Ok(PeekResult::Hole(size)) => {
                            samples
                                .lock()
                                .unwrap()
                                .extend(std::iter::repeat_n(0.0, size / 4));
                        }
                        Err(error) => {
                            eprintln!("audio error: {error}");
                            break;
                        }
                    }
                    if stream.discard().is_err() {
                        break;
                    }
                }
                thread::sleep(Duration::from_millis(5));
            }
            let _ = stream.disconnect();
            context.disconnect();
        });
        let mut capture = Self {
            stop,
            worker: Some(worker),
        };
        match ready_rx.recv_timeout(CONNECT_TIMEOUT * 2 + Duration::from_secs(1)) {
            Ok(Ok(())) => Ok(capture),
            result => {
                capture.stop();
                anyhow::bail!(
                    "{}",
                    result
                        .ok()
                        .and_then(Result::err)
                        .unwrap_or_else(|| "Не удалось запустить запись".into())
                );
            }
        }
    }

    pub fn is_active(&self) -> bool {
        self.worker
            .as_ref()
            .map(|w| !w.is_finished())
            .unwrap_or(false)
    }

    fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.stop();
    }
}
