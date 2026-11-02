// SPDX-License-Identifier: AGPL-3.0-or-later
//! Звуки для кнопок: читает WAV, MP3, OGG (Vorbis) и FLAC и переводит в формат движка —
//! 48 кГц, стерео, float. Файл читается целиком заранее (вне потока реального времени),
//! движок потом только копирует готовые отсчёты.

use std::fmt;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;

use engine::{CHANNELS, RATE};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// Самый длинный звук для кнопки. Дольше — это уже не кнопка, а плеер (и много памяти).
pub const MAX_SECONDS: u32 = 180;

/// Готовый звук: 48 кГц, стерео, отсчёты по очереди (L, R, L, R…).
pub type Clip = Arc<[f32]>;

#[derive(Debug, Clone, PartialEq)]
pub enum ClipError {
    /// Файла нет или нет доступа.
    Missing(String),
    /// Формат не поддерживается или файл повреждён.
    Unsupported(String),
    /// Длиннее MAX_SECONDS.
    TooLong,
}

impl ClipError {
    /// Ключ для интерфейса.
    pub fn code(&self) -> &'static str {
        match self {
            ClipError::Missing(_) => "file-missing",
            ClipError::Unsupported(_) => "file-unsupported",
            ClipError::TooLong => "file-too-long",
        }
    }
}

impl fmt::Display for ClipError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ClipError::Missing(e) => write!(f, "cannot open sound file: {e}"),
            ClipError::Unsupported(e) => write!(f, "unsupported or damaged sound file: {e}"),
            ClipError::TooLong => write!(f, "sound file is longer than {MAX_SECONDS} s"),
        }
    }
}

/// Читает файл целиком и переводит в 48 кГц стерео.
pub fn load(path: &Path) -> Result<Clip, ClipError> {
    let file = File::open(path).map_err(|e| ClipError::Missing(e.to_string()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let unsupported = |e: SymError| ClipError::Unsupported(e.to_string());
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(unsupported)?;
    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| ClipError::Unsupported("no audio track".into()))?;
    let track_id = track.id;
    let mut decoder =
        symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default()).map_err(unsupported)?;

    let mut rate = track.codec_params.sample_rate.unwrap_or(RATE);
    let mut stereo: Vec<f32> = Vec::new();
    let mut buf: Option<SampleBuffer<f32>> = None;
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(SymError::ResetRequired) => break,
            Err(e) => return Err(unsupported(e)),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(SymError::DecodeError(_)) => continue, // битый кусок — пропускаем, как плееры
            Err(e) => return Err(unsupported(e)),
        };
        let spec = *decoded.spec();
        rate = spec.rate;
        let channels = spec.channels.count().max(1);
        let sb = buf.get_or_insert_with(|| SampleBuffer::new(decoded.capacity() as u64, spec));
        if sb.capacity() < decoded.capacity() * channels {
            *sb = SampleBuffer::new(decoded.capacity() as u64, spec);
        }
        sb.copy_interleaved_ref(decoded);
        for frame in sb.samples().chunks_exact(channels) {
            let (l, r) = if channels == 1 { (frame[0], frame[0]) } else { (frame[0], frame[1]) };
            stereo.push(l);
            stereo.push(r);
        }
        if stereo.len() / CHANNELS > (MAX_SECONDS * rate) as usize {
            return Err(ClipError::TooLong);
        }
    }
    if stereo.is_empty() {
        return Err(ClipError::Unsupported("no sound in file".into()));
    }
    Ok(resample(&stereo, rate).into())
}

/// Перевод частоты в 48 кГц (кубическая интерполяция). Для 48 кГц — без изменений.
pub fn resample(stereo: &[f32], from_rate: u32) -> Vec<f32> {
    if from_rate == RATE || from_rate == 0 {
        return stereo.to_vec();
    }
    let frames = stereo.len() / CHANNELS;
    let step = from_rate as f64 / RATE as f64;
    let out_frames = ((frames as f64) / step).floor() as usize;
    let at = |i: isize, ch: usize| -> f32 {
        let i = i.clamp(0, frames as isize - 1) as usize;
        stereo[i * CHANNELS + ch]
    };
    let mut out = Vec::with_capacity(out_frames * CHANNELS);
    for n in 0..out_frames {
        let pos = n as f64 * step;
        let i = pos.floor() as isize;
        let t = (pos - i as f64) as f32;
        for ch in 0..CHANNELS {
            let (y0, y1, y2, y3) = (at(i - 1, ch), at(i, ch), at(i + 1, ch), at(i + 2, ch));
            let c1 = 0.5 * (y2 - y0);
            let c2 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
            let c3 = 0.5 * (y3 - y0) + 1.5 * (y1 - y2);
            out.push(((c3 * t + c2) * t + c1) * t + y1);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;
    use std::io::Write;

    /// Пишет простой 16-бит WAV для проверки.
    fn write_wav(path: &Path, rate: u32, channels: u16, samples: &[i16]) {
        let data_len = (samples.len() * 2) as u32;
        let mut f = File::create(path).unwrap();
        f.write_all(b"RIFF").unwrap();
        f.write_all(&(36 + data_len).to_le_bytes()).unwrap();
        f.write_all(b"WAVEfmt ").unwrap();
        f.write_all(&16u32.to_le_bytes()).unwrap();
        f.write_all(&1u16.to_le_bytes()).unwrap();
        f.write_all(&channels.to_le_bytes()).unwrap();
        f.write_all(&rate.to_le_bytes()).unwrap();
        f.write_all(&(rate * channels as u32 * 2).to_le_bytes()).unwrap();
        f.write_all(&(channels * 2).to_le_bytes()).unwrap();
        f.write_all(&16u16.to_le_bytes()).unwrap();
        f.write_all(b"data").unwrap();
        f.write_all(&data_len.to_le_bytes()).unwrap();
        for s in samples {
            f.write_all(&s.to_le_bytes()).unwrap();
        }
    }

    fn temp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("sb-clips-{}-{name}", std::process::id()))
    }

    #[test]
    fn reads_mono_44100_wav_as_48k_stereo() {
        let p = temp("mono.wav");
        let tone: Vec<i16> = (0..44_100).map(|i| (8000.0 * (i as f32 * TAU * 441.0 / 44_100.0).sin()) as i16).collect();
        write_wav(&p, 44_100, 1, &tone);
        let clip = load(&p).unwrap();
        std::fs::remove_file(&p).ok();
        let frames = clip.len() / 2;
        assert!((frames as i64 - 48_000).abs() < 10, "{frames}");
        assert_eq!(clip[100], clip[101], "моно → одинаковые каналы");
        let peak = clip.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!((peak - 8000.0 / 32768.0).abs() < 0.01);
    }

    #[test]
    fn missing_and_bad_files_are_reported() {
        assert_eq!(load(Path::new("Z:/no/such/file.mp3")).unwrap_err().code(), "file-missing");
        let p = temp("bad.mp3");
        std::fs::write(&p, b"this is not audio").unwrap();
        let e = load(&p).unwrap_err();
        std::fs::remove_file(&p).ok();
        assert_eq!(e.code(), "file-unsupported");
    }

    #[test]
    fn too_long_file_is_rejected() {
        let p = temp("long.wav");
        // 8 кГц, чтобы файл был небольшим: 181 с.
        write_wav(&p, 8000, 1, &vec![0i16; 8000 * (MAX_SECONDS as usize + 1)]);
        let e = load(&p).unwrap_err();
        std::fs::remove_file(&p).ok();
        assert_eq!(e, ClipError::TooLong);
    }

    #[test]
    fn resample_keeps_pitch() {
        let src: Vec<f32> = (0..32_000).flat_map(|i| {
            let v = (i as f32 * TAU * 1000.0 / 32_000.0).sin();
            [v, v]
        })
        .collect();
        let out = resample(&src, 32_000);
        let left: Vec<f32> = out.iter().step_by(2).copied().collect();
        let crossings = left.windows(2).filter(|w| (w[0] < 0.0) != (w[1] < 0.0)).count();
        assert!((crossings as i32 - 2000).abs() <= 4, "{crossings}");
    }
}
