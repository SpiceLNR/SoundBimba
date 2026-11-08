// SPDX-License-Identifier: AGPL-3.0-or-later
//! Звуки для кнопок: WAV, MP3, OGG (Vorbis), FLAC → 48 кГц, стерео, float.
//!
//! Длина файла не ограничена: в памяти — только первые 2 секунды (чтобы звук стартовал
//! мгновенно), остальное читается с диска на ходу отдельным потоком («подкачка») и передаётся
//! движку через кольцевой буфер. Перемотка: движок просит позицию, подкачка перематывает файл
//! и ставит в буфер метку (NaN, номер просьбы), после которой идут отсчёты с новой позиции.

use std::fmt;
use std::fs::File;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use engine::{ButtonSource, TailControl, TailFeed, CHANNELS, RATE};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{Decoder, DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// Сколько секунд звука держать в памяти заранее.
pub const HEAD_SECONDS: u32 = 2;
/// Запас подкачки впереди позиции проигрывания, секунд.
const TAIL_BUFFER_SECONDS: usize = 3;
/// До этой позиции подкачка не перематывает файл, а читает его с начала тем же способом, что и
/// начало в памяти, и пропускает лишнее уже после перевода частоты: так стык «память → подкачка»
/// совпадает до отсчёта. Перемотка сбрасывает декодер (у MP3 звук сразу после неё чуть другой) и
/// фазу перевода частоты — на стыке был бы щелчок.
const SEAMLESS_SECONDS: u64 = 4;

#[derive(Debug, Clone, PartialEq)]
pub enum ClipError {
    /// Файла нет или нет доступа.
    Missing(String),
    /// Формат не поддерживается или файл повреждён.
    Unsupported(String),
}

impl ClipError {
    pub fn code(&self) -> &'static str {
        match self {
            ClipError::Missing(_) => "file-missing",
            ClipError::Unsupported(_) => "file-unsupported",
        }
    }
}

impl fmt::Display for ClipError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ClipError::Missing(e) => write!(f, "cannot open sound file: {e}"),
            ClipError::Unsupported(e) => write!(f, "unsupported or damaged sound file: {e}"),
        }
    }
}

fn unsupported(e: SymError) -> ClipError {
    ClipError::Unsupported(e.to_string())
}

/// Открытый файл: демультиплексор + декодер первой звуковой дорожки.
struct Reader {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track_id: u32,
    rate: u32,
    n_frames: Option<u64>,
    buf: Option<SampleBuffer<f32>>,
}

impl Reader {
    fn open(path: &Path) -> Result<Reader, ClipError> {
        let file = File::open(path).map_err(|e| ClipError::Missing(e.to_string()))?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }
        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
            .map_err(unsupported)?;
        let format = probed.format;
        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .ok_or_else(|| ClipError::Unsupported("no audio track".into()))?;
        let decoder =
            symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default()).map_err(unsupported)?;
        Ok(Reader {
            track_id: track.id,
            rate: track.codec_params.sample_rate.unwrap_or(RATE),
            n_frames: track.codec_params.n_frames,
            format,
            decoder,
            buf: None,
        })
    }

    /// Следующий кусок звука как стерео-кадры с частотой файла. Ok(false) — конец файла.
    fn next_stereo(&mut self, out: &mut Vec<f32>) -> Result<bool, ClipError> {
        loop {
            let packet = match self.format.next_packet() {
                Ok(p) => p,
                Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(false),
                Err(SymError::ResetRequired) => return Ok(false),
                Err(e) => return Err(unsupported(e)),
            };
            if packet.track_id() != self.track_id {
                continue;
            }
            let decoded = match self.decoder.decode(&packet) {
                Ok(d) => d,
                Err(SymError::DecodeError(_)) => continue, // битый кусок — пропускаем, как плееры
                Err(e) => return Err(unsupported(e)),
            };
            let spec = *decoded.spec();
            self.rate = spec.rate;
            let channels = spec.channels.count().max(1);
            let needed = decoded.capacity() * channels;
            if self.buf.as_ref().is_none_or(|b| b.capacity() < needed) {
                self.buf = Some(SampleBuffer::new(decoded.capacity() as u64, spec));
            }
            let sb = self.buf.as_mut().expect("buffer");
            sb.copy_interleaved_ref(decoded);
            for frame in sb.samples().chunks_exact(channels) {
                let (l, r) = if channels == 1 { (frame[0], frame[0]) } else { (frame[0], frame[1]) };
                // NaN в звуке занят под метку перемотки — на всякий случай чистим.
                out.push(if l.is_finite() { l } else { 0.0 });
                out.push(if r.is_finite() { r } else { 0.0 });
            }
            return Ok(true);
        }
    }

    /// Перемотка на кадр файла. Возвращает, сколько кадров ещё отбросить до точной позиции.
    fn seek(&mut self, frame: u64) -> Result<u64, ClipError> {
        let seeked = self
            .format
            .seek(SeekMode::Accurate, SeekTo::TimeStamp { ts: frame, track_id: self.track_id })
            .map_err(unsupported)?;
        self.decoder.reset();
        Ok(seeked.required_ts.saturating_sub(seeked.actual_ts))
    }
}

/// Длина файла в кадрах 48 кГц: из заголовка, а если там нет — по сумме длительностей пакетов.
fn total_frames(path: &Path) -> Result<u64, ClipError> {
    let mut r = Reader::open(path)?;
    let src = match r.n_frames {
        Some(n) => n,
        None => {
            let mut sum = 0u64;
            loop {
                match r.format.next_packet() {
                    Ok(p) if p.track_id() == r.track_id => sum += p.dur(),
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
            sum
        }
    };
    Ok(src * RATE as u64 / r.rate.max(1) as u64)
}

/// Перевод частоты на ходу (кубическая интерполяция), с памятью между кусками.
struct Resampler {
    step: f64,
    pos: f64,
    hist: [[f32; CHANNELS]; 4],
    filled: usize,
}

impl Resampler {
    fn new(from_rate: u32) -> Resampler {
        Resampler { step: from_rate as f64 / RATE as f64, pos: 0.0, hist: [[0.0; CHANNELS]; 4], filled: 0 }
    }

    fn reset(&mut self) {
        self.pos = 0.0;
        self.filled = 0;
    }

    fn push(&mut self, input: &[f32], out: &mut Vec<f32>) {
        if self.step == 1.0 {
            out.extend_from_slice(input);
            return;
        }
        for frame in input.as_chunks::<CHANNELS>().0 {
            self.hist.copy_within(1.., 0);
            self.hist[3] = *frame;
            self.filled += 1;
            if self.filled < 4 {
                continue;
            }
            while self.pos < 1.0 {
                let t = self.pos as f32;
                let [h0, h1, h2, h3] = self.hist;
                for (((&y0, &y1), &y2), &y3) in h0.iter().zip(&h1).zip(&h2).zip(&h3) {
                    let c1 = 0.5 * (y2 - y0);
                    let c2 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
                    let c3 = 0.5 * (y3 - y0) + 1.5 * (y1 - y2);
                    out.push(((c3 * t + c2) * t + c1) * t + y1);
                }
                self.pos += self.step;
            }
            self.pos -= 1.0;
        }
    }
}

/// Проверка, что файл открывается и это звук (быстро, без чтения всего файла).
pub fn probe(path: &Path) -> Result<(), ClipError> {
    Reader::open(path).map(|_| ())
}

/// Поток подкачки. Останавливается при удалении.
pub struct Streamer {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for Streamer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Открывает звук для кнопки: начало — в память, остальное — подкачкой (если файл длиннее).
pub fn open(path: &Path) -> Result<(ButtonSource, Option<Streamer>), ClipError> {
    let total = total_frames(path)?;
    let mut reader = Reader::open(path)?;
    let mut resampler = Resampler::new(reader.rate);
    let head_target = (HEAD_SECONDS * RATE) as usize * CHANNELS;
    let mut head = Vec::with_capacity(head_target + 8192);
    let mut scratch = Vec::new();
    let mut ended = false;
    while head.len() < head_target {
        scratch.clear();
        if !reader.next_stereo(&mut scratch)? {
            ended = true;
            break;
        }
        resampler.push(&scratch, &mut head);
    }
    if ended {
        if head.is_empty() {
            return Err(ClipError::Unsupported("no sound in file".into()));
        }
        let frames = (head.len() / CHANNELS) as u64;
        return Ok((ButtonSource { head: head.into(), total_frames: frames, tail: None }, None));
    }
    head.truncate(head_target);
    let head_frames = (head_target / CHANNELS) as u64;

    let (prod, cons) = rtrb::RingBuffer::new(TAIL_BUFFER_SECONDS * RATE as usize * CHANNELS);
    let ctl = Arc::new(TailControl::default());
    let stop = Arc::new(AtomicBool::new(false));
    let (path_owned, c, s) = (path.to_path_buf(), ctl.clone(), stop.clone());
    let thread = std::thread::Builder::new()
        .name("sb-clip".into())
        .spawn(move || stream(&path_owned, prod, &c, &s))
        .map_err(|e| ClipError::Unsupported(e.to_string()))?;
    Ok((
        ButtonSource { head: head.into(), total_frames: total.max(head_frames), tail: Some(TailFeed { cons, ctl }) },
        Some(Streamer { stop, thread: Some(thread) }),
    ))
}

/// Подкачка: ждёт просьб движка (номер + позиция), перематывает и заполняет буфер впереди.
fn stream(path: &Path, mut prod: rtrb::Producer<f32>, ctl: &TailControl, stop: &AtomicBool) {
    let Ok(mut reader) = Reader::open(path) else { return };
    let mut resampler = Resampler::new(reader.rate);
    let mut my_gen = 0u32;
    let mut pending: Vec<f32> = Vec::new();
    let mut sent = 0usize;
    let mut skip = 0usize;
    // Сколько кадров 48 кГц выбросить после перевода частоты (чтение с начала файла).
    let mut skip_out = 0u64;
    let mut eof = false;
    let mut scratch = Vec::new();
    let wait = || std::thread::sleep(Duration::from_millis(4));
    while !stop.load(Ordering::Relaxed) {
        let g = ctl.want_gen.load(Ordering::Acquire);
        if g != my_gen {
            my_gen = g;
            let pos48 = ctl.want_pos.load(Ordering::Acquire);
            let src = pos48 * reader.rate as u64 / RATE as u64;
            skip_out = 0;
            skip = if pos48 <= SEAMLESS_SECONDS * RATE as u64 {
                // Близко к началу (обычно — сразу за началом в памяти): с начала файла, без перемотки.
                match Reader::open(path) {
                    Ok(r) => reader = r,
                    Err(_) => return,
                }
                skip_out = pos48;
                0
            } else {
                match reader.seek(src) {
                    Ok(extra) => extra as usize,
                    Err(_) => {
                        // Файл не перематывается — читаем сначала и пропускаем до позиции.
                        match Reader::open(path) {
                            Ok(r) => reader = r,
                            Err(_) => return,
                        }
                        src as usize
                    }
                }
            };
            resampler = Resampler::new(reader.rate);
            resampler.reset();
            pending.clear();
            sent = 0;
            eof = false;
            // Метка: дальше — отсчёты по просьбе номер g.
            while prod.slots() < CHANNELS {
                if stop.load(Ordering::Relaxed) || ctl.want_gen.load(Ordering::Acquire) != my_gen {
                    break;
                }
                wait();
            }
            if prod.slots() >= CHANNELS {
                let _ = prod.push(f32::NAN);
                let _ = prod.push(g as f32);
            }
            continue;
        }
        if my_gen == 0 {
            wait();
            continue;
        }
        if sent >= pending.len() {
            pending.clear();
            sent = 0;
            if eof {
                // Всё до конца файла уже в буфере — сообщаем движку, иначе он ждал бы последних
                // кадров, которые есть только в заголовке.
                ctl.eof_gen.store(my_gen, Ordering::Release);
                wait();
                continue;
            }
            scratch.clear();
            match reader.next_stereo(&mut scratch) {
                Ok(true) => {
                    let drop_frames = skip.min(scratch.len() / CHANNELS);
                    skip -= drop_frames;
                    resampler.push(&scratch[drop_frames * CHANNELS..], &mut pending);
                    let drop_out = skip_out.min((pending.len() / CHANNELS) as u64);
                    skip_out -= drop_out;
                    sent = drop_out as usize * CHANNELS;
                }
                Ok(false) | Err(_) => eof = true,
            }
            continue;
        }
        let room = prod.slots() / CHANNELS * CHANNELS;
        let take = room.min(pending.len() - sent);
        if take == 0 {
            wait();
            continue;
        }
        for &v in &pending[sent..sent + take] {
            let _ = prod.push(v);
        }
        sent += take;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;
    use std::io::Write;
    use std::sync::atomic::Ordering::Release;

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
    fn short_file_fits_in_memory_without_streaming() {
        let p = temp("short.wav");
        let tone: Vec<i16> = (0..44_100).map(|i| (8000.0 * (i as f32 * TAU * 441.0 / 44_100.0).sin()) as i16).collect();
        write_wav(&p, 44_100, 1, &tone);
        let (src, streamer) = open(&p).unwrap();
        std::fs::remove_file(&p).ok();
        assert!(streamer.is_none() && src.tail.is_none());
        let frames = src.head.len() / 2;
        assert!((frames as i64 - 48_000).abs() < 10, "{frames}");
        assert_eq!(src.total_frames, frames as u64);
        assert_eq!(src.head[100], src.head[101], "моно → одинаковые каналы");
    }

    #[test]
    fn long_file_streams_from_the_requested_position() {
        // 10 с «лесенки»: значение отсчёта = номер секунды / 10 — по нему видно, откуда идёт звук.
        let p = temp("long.wav");
        let rate = 48_000;
        let samples: Vec<i16> = (0..rate * 10).map(|i| ((i / rate) as i16) * 3000).collect();
        write_wav(&p, rate as u32, 1, &samples);
        let (mut src, streamer) = open(&p).unwrap();
        assert!(streamer.is_some());
        assert_eq!(src.total_frames, 480_000);
        assert_eq!(src.head.len(), (HEAD_SECONDS * RATE) as usize * 2);
        let tail = src.tail.as_mut().unwrap();
        // Просьба: позиция 7,5 с.
        tail.ctl.want_pos.store(360_000, Release);
        tail.ctl.want_gen.store(1, Release);
        let mut got = Vec::new();
        for _ in 0..400 {
            while let Ok(v) = tail.cons.pop() {
                got.push(v);
            }
            if got.len() > 2 + 2000 {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        // Дочитываем до конца файла: подкачка должна сообщить «файл кончился».
        for _ in 0..400 {
            while tail.cons.pop().is_ok() {}
            if tail.ctl.eof_gen.load(std::sync::atomic::Ordering::Acquire) == 1 {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let ctl_eof = tail.ctl.eof_gen.load(std::sync::atomic::Ordering::Acquire);
        drop(streamer);
        std::fs::remove_file(&p).ok();
        assert!(got[0].is_nan() && got[1] == 1.0, "сначала метка перемотки");
        let expected = 7.0 * 3000.0 / 32768.0;
        assert!((got[2] - expected).abs() < 1e-3, "звук с 7-й секунды: {}", got[2]);
        assert_eq!(ctl_eof, 1, "дочитала до конца — сообщила движку");
    }

    #[test]
    fn missing_and_bad_files_are_reported() {
        assert_eq!(open(Path::new("Z:/no/such/file.mp3")).err().map(|e| e.code()), Some("file-missing"));
        let p = temp("bad.mp3");
        std::fs::write(&p, b"this is not audio").unwrap();
        let e = probe(&p).unwrap_err();
        std::fs::remove_file(&p).ok();
        assert_eq!(e.code(), "file-unsupported");
    }

    #[test]
    fn streaming_resampler_keeps_pitch() {
        let src: Vec<f32> = (0..32_000)
            .flat_map(|i| {
                let v = (i as f32 * TAU * 1000.0 / 32_000.0).sin();
                [v, v]
            })
            .collect();
        let mut r = Resampler::new(32_000);
        let mut out = Vec::new();
        for chunk in src.chunks(1152 * 2) {
            r.push(chunk, &mut out);
        }
        let left: Vec<f32> = out.iter().step_by(2).copied().collect();
        assert!((left.len() as i64 - 48_000).abs() < 10);
        let crossings = left.windows(2).filter(|w| (w[0] < 0.0) != (w[1] < 0.0)).count();
        assert!((crossings as i32 - 2000).abs() <= 4, "{crossings}");
    }
}
