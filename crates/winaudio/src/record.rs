// SPDX-License-Identifier: AGPL-3.0-or-later
//! Короткая запись с микрофона (подстройка морфера под голос): моно, 48 кГц. Поток тот же,
//! что у движка; звук никуда не уходит и не сохраняется — только для замера.

use std::time::{Duration, Instant};

use engine::{BLOCK, BLOCK_SAMPLES, RATE};

use crate::streams::{open_capture, Source};

/// Записывает `seconds` секунд с устройства захвата `device_id`.
pub fn record_mic(device_id: &str, seconds: f32) -> Result<Vec<f32>, String> {
    let (stream, mut reader) = open_capture(Source::Device(device_id.to_string()), 20.0)?;
    let blocks = (seconds * RATE as f32 / BLOCK as f32) as usize;
    let mut block = vec![0.0f32; BLOCK_SAMPLES];
    let mut mono = Vec::with_capacity(blocks * BLOCK);
    let start = Instant::now();
    for b in 0..blocks {
        reader.read(&mut block);
        mono.extend(block.chunks(2).map(|f| (f[0] + f[1]) * 0.5));
        let due = start + Duration::from_secs_f64((b + 1) as f64 * BLOCK as f64 / RATE as f64);
        if let Some(w) = due.checked_duration_since(Instant::now()) {
            std::thread::sleep(w);
        }
    }
    match stream.failed() {
        Some(e) => Err(e),
        None => Ok(mono),
    }
}
