// SPDX-License-Identifier: AGPL-3.0-or-later
//! Разбор формата звука WAVEFORMATEX и перевод сэмплов в float и обратно (общий для пробы и
//! приложения).

use std::ptr::read_unaligned;

use windows::Win32::Media::Audio::{WAVEFORMATEX, WAVEFORMATEXTENSIBLE, WAVE_FORMAT_PCM};
use windows::Win32::Media::KernelStreaming::{KSDATAFORMAT_SUBTYPE_PCM, WAVE_FORMAT_EXTENSIBLE};
use windows::Win32::Media::Multimedia::{KSDATAFORMAT_SUBTYPE_IEEE_FLOAT, WAVE_FORMAT_IEEE_FLOAT};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sample {
    F32,
    I16,
    I24,
    I32,
    Unknown,
}

#[derive(Clone, Copy, Debug)]
pub struct Fmt {
    pub rate: u32,
    pub channels: u16,
    pub bits: u16,
    pub valid_bits: u16,
    pub block_align: u16,
    pub sample: Sample,
}

impl Fmt {
    /// Читает формат по указателю, который вернула Windows.
    ///
    /// # Safety
    /// `p` должен указывать на корректный WAVEFORMATEX (или WAVEFORMATEXTENSIBLE).
    pub unsafe fn from_ptr(p: *const WAVEFORMATEX) -> Fmt {
        let wf = unsafe { read_unaligned(p) };
        let tag = wf.wFormatTag as u32;
        let bits = wf.wBitsPerSample;
        let mut valid_bits = bits;
        let mut is_float = tag == WAVE_FORMAT_IEEE_FLOAT;
        let mut is_pcm = tag == WAVE_FORMAT_PCM;
        if tag == WAVE_FORMAT_EXTENSIBLE && wf.cbSize >= 22 {
            let ext = unsafe { read_unaligned(p as *const WAVEFORMATEXTENSIBLE) };
            let sub = { ext.SubFormat };
            let vb = unsafe { ext.Samples.wValidBitsPerSample };
            if vb != 0 {
                valid_bits = vb;
            }
            is_float = sub == KSDATAFORMAT_SUBTYPE_IEEE_FLOAT;
            is_pcm = sub == KSDATAFORMAT_SUBTYPE_PCM;
        }
        let sample = match (is_float, is_pcm, bits) {
            (true, _, 32) => Sample::F32,
            (_, true, 16) => Sample::I16,
            (_, true, 24) => Sample::I24,
            (_, true, 32) => Sample::I32,
            _ => Sample::Unknown,
        };
        Fmt {
            rate: wf.nSamplesPerSec,
            channels: wf.nChannels,
            bits,
            valid_bits,
            block_align: wf.nBlockAlign,
            sample,
        }
    }

    pub fn describe(&self) -> String {
        let kind = match self.sample {
            Sample::F32 => "float".to_string(),
            Sample::Unknown => "неизвестный тип".to_string(),
            _ if self.valid_bits != self.bits => format!("целые, {} знач. бит", self.valid_bits),
            _ => "целые".to_string(),
        };
        format!(
            "{} Гц, {} кан., {} бит ({kind})",
            self.rate, self.channels, self.bits
        )
    }

    /// Дописывает кадры из сырого буфера в `out` как float (все каналы, по очереди).
    pub fn push_f32(&self, bytes: &[u8], out: &mut Vec<f32>) {
        match self.sample {
            Sample::F32 => out.extend(bytes.as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes(*b))),
            Sample::I16 => out.extend(
                bytes.as_chunks::<2>().0.iter().map(|b| i16::from_le_bytes(*b) as f32 / 32768.0),
            ),
            Sample::I24 => out.extend(bytes.as_chunks::<3>().0.iter().map(|b| {
                (i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8) as f32 / 8_388_608.0
            })),
            Sample::I32 => out.extend(
                bytes.as_chunks::<4>().0.iter().map(|b| i32::from_le_bytes(*b) as f32 / 2_147_483_648.0),
            ),
            Sample::Unknown => {
                let n = bytes.len() / (self.bits as usize / 8).max(1);
                out.extend(std::iter::repeat_n(0.0, n));
            }
        }
    }

    /// Записывает одно значение во все каналы кадра `frame` буфера `out`.
    pub fn write_frame(&self, out: &mut [u8], frame: usize, value: f32) {
        for ch in 0..self.channels as usize {
            self.write_sample(out, frame, ch, value);
        }
    }

    /// Записывает одно значение в канал `ch` кадра `frame`.
    pub fn write_sample(&self, out: &mut [u8], frame: usize, ch: usize, value: f32) {
        let v = value.clamp(-1.0, 1.0);
        let bps = self.bits as usize / 8;
        let at = frame * self.block_align as usize + ch * bps;
        let dst = &mut out[at..at + bps];
        match self.sample {
            Sample::F32 => dst.copy_from_slice(&v.to_le_bytes()),
            Sample::I16 => dst.copy_from_slice(&((v * 32767.0) as i16).to_le_bytes()),
            Sample::I24 => dst.copy_from_slice(&((v * 8_388_607.0) as i32).to_le_bytes()[..3]),
            Sample::I32 => dst.copy_from_slice(&((v as f64 * 2_147_483_647.0) as i32).to_le_bytes()),
            Sample::Unknown => dst.fill(0),
        }
    }

    /// Кадры устройства → стерео float: моно раздваивается, лишние каналы отбрасываются.
    /// `scratch` — заранее выделенный буфер (поток реального времени не выделяет память).
    pub fn to_stereo(&self, bytes: &[u8], scratch: &mut Vec<f32>, out: &mut Vec<f32>) {
        scratch.clear();
        self.push_f32(bytes, scratch);
        let ch = self.channels.max(1) as usize;
        for frame in scratch.chunks_exact(ch) {
            let (l, r) = if ch == 1 { (frame[0], frame[0]) } else { (frame[0], frame[1]) };
            out.push(l);
            out.push(r);
        }
    }

    /// Стерео float → кадры устройства: для моно — среднее, в лишние каналы — тишина.
    pub fn from_stereo(&self, stereo: &[f32], out: &mut [u8]) {
        let ch = self.channels as usize;
        for (f, &[l, r]) in stereo.as_chunks::<2>().0.iter().enumerate() {
            if ch == 1 {
                self.write_sample(out, f, 0, 0.5 * (l + r));
                continue;
            }
            for c in 0..ch {
                let v = match c {
                    0 => l,
                    1 => r,
                    _ => 0.0,
                };
                self.write_sample(out, f, c, v);
            }
        }
    }
}

/// Формат 32-бит float для своих потоков (захват процесса).
pub fn float_format(rate: u32, channels: u16) -> WAVEFORMATEX {
    WAVEFORMATEX {
        wFormatTag: WAVE_FORMAT_IEEE_FLOAT as u16,
        nChannels: channels,
        nSamplesPerSec: rate,
        nAvgBytesPerSec: rate * channels as u32 * 4,
        nBlockAlign: channels * 4,
        wBitsPerSample: 32,
        cbSize: 0,
    }
}

/// Формат 16-бит PCM (как в примере Microsoft ApplicationLoopback) — запасной вариант.
pub fn pcm16_format(rate: u32, channels: u16) -> WAVEFORMATEX {
    WAVEFORMATEX {
        wFormatTag: WAVE_FORMAT_PCM as u16,
        nChannels: channels,
        nSamplesPerSec: rate,
        nAvgBytesPerSec: rate * channels as u32 * 2,
        nBlockAlign: channels * 2,
        wBitsPerSample: 16,
        cbSize: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn i16_roundtrip() {
        let f = unsafe { Fmt::from_ptr(&pcm16_format(48000, 2)) };
        assert_eq!(f.sample, Sample::I16);
        let mut buf = vec![0u8; 4];
        f.write_frame(&mut buf, 0, 0.5);
        let mut out = Vec::new();
        f.push_f32(&buf, &mut out);
        assert_eq!(out.len(), 2);
        assert!((out[0] - 0.5).abs() < 1e-3 && (out[1] - 0.5).abs() < 1e-3);
    }

    #[test]
    fn mono_and_multichannel_convert_to_stereo() {
        let mono = unsafe { Fmt::from_ptr(&float_format(48000, 1)) };
        let bytes: Vec<u8> = [0.25f32, -0.5].iter().flat_map(|v| v.to_le_bytes()).collect();
        let (mut scratch, mut out) = (Vec::new(), Vec::new());
        mono.to_stereo(&bytes, &mut scratch, &mut out);
        assert_eq!(out, vec![0.25, 0.25, -0.5, -0.5]);

        let quad = unsafe { Fmt::from_ptr(&float_format(48000, 4)) };
        let mut dev = vec![0u8; 2 * 16];
        quad.from_stereo(&[0.1, 0.2, 0.3, 0.4], &mut dev);
        let mut back = Vec::new();
        quad.push_f32(&dev, &mut back);
        assert_eq!(back, vec![0.1, 0.2, 0.0, 0.0, 0.3, 0.4, 0.0, 0.0]);
    }

    #[test]
    fn float_describe() {
        let f = unsafe { Fmt::from_ptr(&float_format(48000, 2)) };
        assert_eq!(f.sample, Sample::F32);
        assert_eq!(f.describe(), "48000 Гц, 2 кан., 32 бит (float)");
    }
}
