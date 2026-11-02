// SPDX-License-Identifier: AGPL-3.0-or-later
//! Потоки звуковых устройств: захват (микрофон или звук процесса) и вывод (наушники, кабель).
//! Каждый поток — свой поток ОС, событийный WASAPI в общем (shared) режиме, формат устройства.
//! С движком поток связан кольцевым буфером из `engine::ring`.
//!
//! Внутри цикла потока память не выделяется: буферы заводятся заранее по размеру буфера устройства.

use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use engine::{ring, PortStats, RingReader, RingWriter, RATE};
use windows::core::{Interface, Result as WinResult, HSTRING};
use windows::Win32::Media::Audio::{
    IAudioCaptureClient, IAudioClient, IAudioClient3, IAudioRenderClient, IMMDeviceEnumerator, MMDeviceEnumerator,
    AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
    AUDCLNT_STREAMFLAGS_EVENTCALLBACK, AUDCLNT_STREAMFLAGS_LOOPBACK, AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
};
use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_ALL};

use crate::devices::ensure_com;
use crate::format::{float_format, Fmt};
use crate::loopback::{activate_process_loopback, EventHandle};

/// Откуда захватывать.
#[derive(Debug, Clone)]
pub enum Source {
    /// Устройство ввода по id Windows.
    Device(String),
    /// Звук процесса (и его дочерних) по PID.
    Process(u32),
}

#[derive(Debug, Clone)]
pub struct StreamInfo {
    pub rate: u32,
    pub format: String,
    /// Размер буфера устройства, кадров.
    pub buffer_frames: u32,
    /// Запас в кольцевом буфере, мс (добавленная задержка).
    pub reserve_ms: f64,
}

/// Работающий поток устройства. Остановка — при удалении.
pub struct Stream {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    pub info: StreamInfo,
    /// Ошибка, из-за которой поток остановился (например, устройство отключили).
    pub error: Arc<Mutex<Option<String>>>,
    pub stats: Arc<PortStats>,
}

impl Stream {
    pub fn failed(&self) -> Option<String> {
        self.error.lock().unwrap().clone()
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        self.stop.store(true, Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn describe(e: &windows::core::Error) -> String {
    format!("{} (0x{:08X})", e.message().trim(), e.code().0 as u32)
}

type Ready<T> = SyncSender<Result<(StreamInfo, T, Arc<PortStats>), String>>;

/// Запускает поток и ждёт, пока он откроет устройство (или сообщит об ошибке).
fn spawn<T: Send + 'static>(
    name: &str,
    body: impl FnOnce(Arc<AtomicBool>, Arc<Mutex<Option<String>>>, Ready<T>) + Send + 'static,
) -> Result<(Stream, T), String> {
    let stop = Arc::new(AtomicBool::new(false));
    let error = Arc::new(Mutex::new(None));
    let (tx, rx) = sync_channel(1);
    let (s, e) = (stop.clone(), error.clone());
    let thread = std::thread::Builder::new()
        .name(name.into())
        .spawn(move || body(s, e, tx))
        .map_err(|e| e.to_string())?;
    match rx.recv_timeout(Duration::from_secs(10)) {
        Ok(Ok((info, end, stats))) => Ok((Stream { stop, thread: Some(thread), info, error, stats }, end)),
        Ok(Err(e)) => {
            let _ = thread.join();
            Err(e)
        }
        Err(_) => {
            stop.store(true, Relaxed);
            Err("audio device did not open in 10 seconds".into())
        }
    }
}

fn device_client(id: &str) -> WinResult<IAudioClient> {
    unsafe {
        let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        en.GetDevice(&HSTRING::from(id))?.Activate::<IAudioClient>(CLSCTX_ALL, None)
    }
}

/// Открывает клиент в формате устройства (shared, событийный). Возвращает формат.
/// Если устройство умеет порции меньше обычных 10 мс (IAudioClient3, Windows 10+), берём
/// самую маленькую — это заметно снижает задержку. Иначе — обычный режим.
fn init_shared(client: &IAudioClient, extra_flags: u32) -> WinResult<Fmt> {
    unsafe {
        let mix = client.GetMixFormat()?;
        let fmt = Fmt::from_ptr(mix);
        let flags = AUDCLNT_STREAMFLAGS_EVENTCALLBACK | extra_flags;
        let low_latency = client.cast::<IAudioClient3>().ok().and_then(|c3| {
            let (mut def, mut fund, mut min, mut max) = (0u32, 0u32, 0u32, 0u32);
            c3.GetSharedModeEnginePeriod(mix, &mut def, &mut fund, &mut min, &mut max).ok()?;
            (min > 0 && min < def).then_some((c3, min))
        });
        let r = match low_latency {
            Some((c3, period)) => c3.InitializeSharedAudioStream(flags, period, mix, None),
            None => client.Initialize(AUDCLNT_SHAREMODE_SHARED, flags, 0, 0, mix, None),
        };
        CoTaskMemFree(Some(mix as _));
        r.map(|_| fmt)
    }
}

/// Захват: звук идёт в кольцевой буфер, движок читает его через возвращённый `RingReader`.
pub fn open_capture(source: Source, reserve_ms: f64) -> Result<(Stream, RingReader), String> {
    spawn("sb-capture", move |stop, error, ready| {
        let opened = (|| -> WinResult<_> {
            ensure_com()?;
            let (client, fmt) = match &source {
                Source::Device(id) => {
                    let c = device_client(id)?;
                    let f = init_shared(&c, 0)?;
                    (c, f)
                }
                Source::Process(pid) => {
                    // У захвата процесса нет своего формата — задаём 48 кГц float стерео.
                    let c = activate_process_loopback(*pid)?;
                    let wf = float_format(RATE, 2);
                    unsafe {
                        c.Initialize(
                            AUDCLNT_SHAREMODE_SHARED,
                            AUDCLNT_STREAMFLAGS_LOOPBACK
                                | AUDCLNT_STREAMFLAGS_EVENTCALLBACK
                                | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
                                | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                            200_000,
                            0,
                            &wf,
                            None,
                        )?
                    };
                    (c, unsafe { Fmt::from_ptr(&wf) })
                }
            };
            let event = EventHandle::new()?;
            unsafe { client.SetEventHandle(event.0)? };
            let cap: IAudioCaptureClient = unsafe { client.GetService()? };
            let buffer = unsafe { client.GetBufferSize() }.unwrap_or(fmt.rate / 10).max(fmt.rate / 100);
            Ok((client, fmt, event, cap, buffer))
        })();
        let (client, fmt, event, cap, buffer) = match opened {
            Ok(v) => v,
            Err(e) => {
                let _ = ready.send(Err(describe(&e)));
                return;
            }
        };
        let reserve = (reserve_ms / 1000.0 * fmt.rate as f64) as usize;
        let (mut writer, reader) = ring(fmt.rate as usize, fmt.rate, RATE, reserve);
        let stats = writer.stats().clone();
        let info = StreamInfo { rate: fmt.rate, format: fmt.describe(), buffer_frames: buffer, reserve_ms };
        if ready.send(Ok((info, reader, stats))).is_err() {
            return;
        }
        let mut scratch = Vec::with_capacity(buffer as usize * fmt.channels.max(1) as usize);
        let mut stereo = Vec::with_capacity(buffer as usize * 2);
        let run = (|| -> WinResult<()> {
            unsafe { client.Start()? };
            while !stop.load(Relaxed) {
                if !event.wait(100) {
                    continue;
                }
                unsafe {
                    while cap.GetNextPacketSize()? > 0 {
                        let (mut data, mut frames, mut flags) = (std::ptr::null_mut(), 0u32, 0u32);
                        cap.GetBuffer(&mut data, &mut frames, &mut flags, None, None)?;
                        stereo.clear();
                        if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || data.is_null() {
                            stereo.resize(frames as usize * 2, 0.0);
                        } else {
                            let bytes = std::slice::from_raw_parts(data, frames as usize * fmt.block_align as usize);
                            fmt.to_stereo(bytes, &mut scratch, &mut stereo);
                        }
                        writer.write(&stereo);
                        cap.ReleaseBuffer(frames)?;
                    }
                }
            }
            Ok(())
        })();
        let _ = unsafe { client.Stop() };
        if let Err(e) = run {
            *error.lock().unwrap() = Some(describe(&e));
        }
    })
}

/// Вывод на устройство: движок пишет в возвращённый `RingWriter`, поток отдаёт звук Windows.
pub fn open_render(device_id: &str, reserve_ms: f64) -> Result<(Stream, RingWriter), String> {
    let id = device_id.to_string();
    spawn("sb-render", move |stop, error, ready| {
        let opened = (|| -> WinResult<_> {
            ensure_com()?;
            let client = device_client(&id)?;
            let fmt = init_shared(&client, 0)?;
            let event = EventHandle::new()?;
            unsafe { client.SetEventHandle(event.0)? };
            let rc: IAudioRenderClient = unsafe { client.GetService()? };
            let buffer = unsafe { client.GetBufferSize()? };
            Ok((client, fmt, event, rc, buffer))
        })();
        let (client, fmt, event, rc, buffer) = match opened {
            Ok(v) => v,
            Err(e) => {
                let _ = ready.send(Err(describe(&e)));
                return;
            }
        };
        let reserve = (reserve_ms / 1000.0 * RATE as f64) as usize;
        let (writer, mut reader) = ring(RATE as usize, RATE, fmt.rate, reserve);
        let stats = writer.stats().clone();
        let info = StreamInfo { rate: fmt.rate, format: fmt.describe(), buffer_frames: buffer, reserve_ms };
        if ready.send(Ok((info, writer, stats))).is_err() {
            return;
        }
        let mut stereo = vec![0.0f32; buffer as usize * 2];
        let run = (|| -> WinResult<()> {
            unsafe {
                // Сначала тишина на весь буфер — без щелчка при старте.
                rc.GetBuffer(buffer)?;
                rc.ReleaseBuffer(buffer, AUDCLNT_BUFFERFLAGS_SILENT.0 as u32)?;
                client.Start()?;
            }
            while !stop.load(Relaxed) {
                if !event.wait(100) {
                    continue;
                }
                unsafe {
                    let free = buffer - client.GetCurrentPadding()?;
                    if free == 0 {
                        continue;
                    }
                    let n = free as usize;
                    reader.read(&mut stereo[..n * 2]);
                    let data = rc.GetBuffer(free)?;
                    let bytes = std::slice::from_raw_parts_mut(data, n * fmt.block_align as usize);
                    fmt.from_stereo(&stereo[..n * 2], bytes);
                    rc.ReleaseBuffer(free, 0)?;
                }
            }
            Ok(())
        })();
        let _ = unsafe { client.Stop() };
        if let Err(e) = run {
            *error.lock().unwrap() = Some(describe(&e));
        }
    })
}
