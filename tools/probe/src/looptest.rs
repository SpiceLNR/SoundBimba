// SPDX-License-Identifier: AGPL-3.0-or-later
//! Шаг 7: тон 440 Гц в виртуальный динамик и одновременная запись с виртуального микрофона.
//! Задержка — по моменту появления тона (время Windows QPC с обеих сторон).

use std::f32::consts::TAU;
use std::time::{Duration, Instant};

use windows::core::{Result, HSTRING};
use windows::Win32::Media::Audio::{
    IAudioCaptureClient, IAudioClient, IAudioRenderClient, IMMDevice,
    AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY, AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED,
    AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
};
use windows::Win32::System::Com::{CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED};
use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};

use crate::capture::EventHandle;
use crate::devices::enumerator;
use crate::format::Fmt;

pub const LEAD_IN: f64 = 0.5;
pub const TONE: f64 = 5.0;
pub const TAIL: f64 = 1.5;
pub const FREQ: f32 = 440.0;
const AMPLITUDE: f32 = 0.5;

/// Текущее время QPC в 100-нс единицах (как pu64QPCPosition у захвата).
fn qpc_100ns() -> i64 {
    let (mut c, mut f) = (0i64, 0i64);
    unsafe {
        let _ = QueryPerformanceCounter(&mut c);
        let _ = QueryPerformanceFrequency(&mut f);
    }
    (c as i128 * 10_000_000 / f.max(1) as i128) as i64
}

fn open(dev: &IMMDevice) -> Result<(IAudioClient, Fmt, EventHandle)> {
    unsafe {
        let client = dev.Activate::<IAudioClient>(CLSCTX_ALL, None)?;
        let mix = client.GetMixFormat()?;
        let fmt = Fmt::from_ptr(mix);
        let r = client.Initialize(AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_EVENTCALLBACK, 0, 0, mix, None);
        CoTaskMemFree(Some(mix as _));
        r?;
        let ev = EventHandle::new()?;
        client.SetEventHandle(ev.0)?;
        Ok((client, fmt, ev))
    }
}

pub struct RenderResult {
    pub fmt: Fmt,
    /// Когда первый кадр тона должен прозвучать (QPC, 100 нс).
    pub onset_qpc: Option<i64>,
    pub underruns: u32,
}

fn render(device_id: &str) -> Result<RenderResult> {
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        let dev = enumerator()?.GetDevice(&HSTRING::from(device_id))?;
        let (client, fmt, ev) = open(&dev)?;
        let rc: IAudioRenderClient = client.GetService()?;
        let size = client.GetBufferSize()?;
        // Сначала заполняем буфер тишиной, чтобы не было щелчка.
        rc.GetBuffer(size)?;
        rc.ReleaseBuffer(size, AUDCLNT_BUFFERFLAGS_SILENT.0 as u32)?;

        let rate = fmt.rate as f64;
        let tone_from = (LEAD_IN * rate) as u64;
        let tone_to = ((LEAD_IN + TONE) * rate) as u64;
        let total = ((LEAD_IN + TONE + 0.3) * rate) as u64;
        let step = TAU * FREQ / fmt.rate as f32;
        let mut pos = 0u64;
        let mut res = RenderResult { fmt, onset_qpc: None, underruns: 0 };

        client.Start()?;
        while pos < total {
            if !ev.wait(500) {
                res.underruns += 1;
            }
            let padding = client.GetCurrentPadding()?;
            let free = size - padding;
            if free == 0 {
                continue;
            }
            let now = qpc_100ns();
            let buf = rc.GetBuffer(free)?;
            let bytes = std::slice::from_raw_parts_mut(buf, free as usize * fmt.block_align as usize);
            for i in 0..free as usize {
                let p = pos + i as u64;
                let v = if (tone_from..tone_to).contains(&p) {
                    if p == tone_from {
                        let ahead = (padding as f64 + i as f64) / rate;
                        res.onset_qpc = Some(now + (ahead * 10_000_000.0) as i64);
                    }
                    AMPLITUDE * ((p - tone_from) as f32 * step).sin()
                } else {
                    0.0
                };
                fmt.write_frame(bytes, i, v);
            }
            rc.ReleaseBuffer(free, 0)?;
            pos += free as u64;
        }
        // Дать доиграть остаток буфера.
        std::thread::sleep(Duration::from_millis(200));
        let _ = client.Stop();
        Ok(res)
    }
}

pub struct LoopResult {
    pub render: RenderResult,
    pub mic_fmt: Fmt,
    /// Первый канал записи с микрофона.
    pub mono: Vec<f32>,
    /// Все каналы записи (для WAV).
    pub all: Vec<f32>,
    /// Время (QPC, 100 нс) каждого пакета: (индекс первого кадра, время).
    pub stamps: Vec<(usize, i64)>,
    pub discontinuities: u32,
    pub silent_packets: u32,
}

pub fn run(speaker_id: &str, mic: &IMMDevice) -> Result<LoopResult> {
    let (client, mic_fmt, ev) = open(mic)?;
    let cap: IAudioCaptureClient = unsafe { client.GetService()? };
    let ch = mic_fmt.channels.max(1) as usize;
    let secs = LEAD_IN + TONE + TAIL;
    let cap_frames = (secs * mic_fmt.rate as f64) as usize + mic_fmt.rate as usize;
    let mut out = LoopResult {
        render: RenderResult { fmt: mic_fmt, onset_qpc: None, underruns: 0 },
        mic_fmt,
        mono: Vec::with_capacity(cap_frames),
        all: Vec::with_capacity(cap_frames * ch),
        stamps: Vec::with_capacity(4096),
        discontinuities: 0,
        silent_packets: 0,
    };

    unsafe { client.Start()? };
    let speaker_id = speaker_id.to_string();
    let player = std::thread::spawn(move || render(&speaker_id));

    let end = Instant::now() + Duration::from_secs_f64(secs);
    let mut tmp = Vec::with_capacity(4096);
    let result = (|| -> Result<()> {
        while Instant::now() < end || (!player.is_finished() && Instant::now() < end + Duration::from_secs(5)) {
            ev.wait(500);
            unsafe {
                while cap.GetNextPacketSize()? > 0 {
                    let (mut data, mut frames, mut flags, mut qpc) = (std::ptr::null_mut(), 0u32, 0u32, 0u64);
                    cap.GetBuffer(&mut data, &mut frames, &mut flags, None, Some(&mut qpc))?;
                    out.stamps.push((out.mono.len(), qpc as i64));
                    tmp.clear();
                    if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || data.is_null() {
                        out.silent_packets += 1;
                        tmp.resize(frames as usize * ch, 0.0);
                    } else {
                        let n = frames as usize * mic_fmt.block_align as usize;
                        mic_fmt.push_f32(std::slice::from_raw_parts(data, n), &mut tmp);
                    }
                    if flags & AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY.0 as u32 != 0 {
                        out.discontinuities += 1;
                    }
                    cap.ReleaseBuffer(frames)?;
                    out.mono.extend(tmp.chunks_exact(ch).map(|f| f[0]));
                    out.all.extend_from_slice(&tmp);
                }
            }
        }
        Ok(())
    })();
    let _ = unsafe { client.Stop() };
    out.render = player
        .join()
        .unwrap_or_else(|_| Err(windows::core::Error::from(windows::Win32::Foundation::E_FAIL)))?;
    result?;
    Ok(out)
}

pub struct Analysis {
    pub peak: f32,
    pub reached: bool,
    pub freq: Option<f32>,
    pub latency_ms: Option<f64>,
}

/// Ищет начало тона, частоту (по переходам через ноль) и задержку.
pub fn analyze(r: &LoopResult) -> Analysis {
    analyze_parts(&r.mono, r.mic_fmt.rate, &r.stamps, r.render.onset_qpc)
}

fn analyze_parts(mono: &[f32], rate: u32, stamps: &[(usize, i64)], onset_qpc: Option<i64>) -> Analysis {
    let peak = mono.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    let reached = peak > 0.01;
    let mut a = Analysis { peak, reached, freq: None, latency_ms: None };
    if !reached {
        return a;
    }
    let Some(onset) = mono.iter().position(|s| s.abs() > peak * 0.25) else {
        return a;
    };
    let win = &mono[onset..mono.len().min(onset + rate as usize)];
    if win.len() > rate as usize / 4 {
        let crossings = win.windows(2).filter(|w| (w[0] < 0.0) != (w[1] < 0.0)).count();
        a.freq = Some(crossings as f32 / 2.0 * rate as f32 / win.len() as f32);
    }
    if let (Some(play), Some(&(idx, t))) = (onset_qpc, stamps.iter().rev().find(|(i, _)| *i <= onset)) {
        if t != 0 {
            let heard = t + ((onset - idx) as f64 * 10_000_000.0 / rate as f64) as i64;
            a.latency_ms = Some((heard - play) as f64 / 10_000.0);
        }
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_tone_and_latency() {
        let rate = 48000;
        // Тишина 0.1 с, затем тон 440 Гц на 0.5 с.
        let mut mono = vec![0.0f32; 4800];
        mono.extend((0..24000).map(|i| 0.5 * (i as f32 * TAU * 440.0 / rate as f32).sin()));
        // Запись началась в момент 1 000 000 (100 нс), тон «сыгран» в момент 1 000 000 + 0.08 с.
        let a = analyze_parts(&mono, rate, &[(0, 1_000_000)], Some(1_000_000 + 800_000));
        assert!(a.reached);
        assert!((a.freq.unwrap() - 440.0).abs() < 5.0);
        // Тон в записи на 0.1 с, сыгран на 0.08 с → задержка ≈ 20 мс.
        assert!((a.latency_ms.unwrap() - 20.0).abs() < 1.0);
    }

    #[test]
    fn silence_is_not_reached() {
        let a = analyze_parts(&[0.0; 48000], 48000, &[(0, 1)], Some(1));
        assert!(!a.reached && a.latency_ms.is_none());
    }
}
