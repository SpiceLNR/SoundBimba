// SPDX-License-Identifier: AGPL-3.0-or-later
//! Запись WAV-файла (32-бит float), без сторонних библиотек.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

pub fn write_f32(path: &Path, rate: u32, channels: u16, samples: &[f32]) -> std::io::Result<()> {
    let data_len = (samples.len() * 4) as u32;
    let frames = samples.len() as u32 / channels.max(1) as u32;
    let mut w = BufWriter::new(File::create(path)?);
    w.write_all(b"RIFF")?;
    w.write_all(&(4 + (8 + 18) + (8 + 4) + 8 + data_len).to_le_bytes())?;
    w.write_all(b"WAVEfmt ")?;
    w.write_all(&18u32.to_le_bytes())?;
    w.write_all(&3u16.to_le_bytes())?; // WAVE_FORMAT_IEEE_FLOAT
    w.write_all(&channels.to_le_bytes())?;
    w.write_all(&rate.to_le_bytes())?;
    w.write_all(&(rate * channels as u32 * 4).to_le_bytes())?;
    w.write_all(&(channels * 4).to_le_bytes())?;
    w.write_all(&32u16.to_le_bytes())?;
    w.write_all(&0u16.to_le_bytes())?;
    w.write_all(b"fact")?;
    w.write_all(&4u32.to_le_bytes())?;
    w.write_all(&frames.to_le_bytes())?;
    w.write_all(b"data")?;
    w.write_all(&data_len.to_le_bytes())?;
    for s in samples {
        w.write_all(&s.to_le_bytes())?;
    }
    w.flush()
}

#[cfg(test)]
mod tests {
    #[test]
    fn header_size() {
        let p = std::env::temp_dir().join("soundbimba-probe-test.wav");
        super::write_f32(&p, 48000, 2, &[0.0; 8]).unwrap();
        let b = std::fs::read(&p).unwrap();
        std::fs::remove_file(&p).ok();
        assert_eq!(b.len(), 12 + 26 + 12 + 8 + 32);
        let riff = u32::from_le_bytes([b[4], b[5], b[6], b[7]]) as usize;
        assert_eq!(riff + 8, b.len());
    }
}
