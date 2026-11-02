// SPDX-License-Identifier: AGPL-3.0-or-later
//! Разбор формата звука WAVEFORMATEX и перевод сэмплов в float и обратно.

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
        let v = value.clamp(-1.0, 1.0);
        let bps = self.bits as usize / 8;
        for ch in 0..self.channels as usize {
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
    fn float_describe() {
        let f = unsafe { Fmt::from_ptr(&float_format(48000, 2)) };
        assert_eq!(f.sample, Sample::F32);
        assert_eq!(f.describe(), "48000 Гц, 2 кан., 32 бит (float)");
    }
}
