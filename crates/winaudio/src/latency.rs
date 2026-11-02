// SPDX-License-Identifier: AGPL-3.0-or-later
//! Замер задержки «SoundBimba → программа, которая читает виртуальный микрофон».
//!
//! Путь тот же, что у движка: кольцевой буфер → поток вывода → «динамик» кабеля → кабель →
//! его «микрофон» → поток захвата → кольцевой буфер. Короткая посылка тона 1 кГц уходит в
//! вывод; задержка — сколько блоков прошло, пока её не услышал захват.

use std::f32::consts::TAU;
use std::time::{Duration, Instant};

use engine::{BLOCK, BLOCK_SAMPLES, RATE};

use crate::streams::{open_capture, open_render, Source};

const FREQ: f32 = 1000.0;
const LEAD_BLOCKS: usize = 60; // 300 мс тишины перед посылкой
const BURST_BLOCKS: usize = 4; // 20 мс тона
const TOTAL_BLOCKS: usize = 300; // 1,5 с

/// Амплитуда частоты `freq` в окне (алгоритм Гёрцеля).
fn tone_amplitude(x: &[f32], freq: f32) -> f32 {
    let coeff = 2.0 * (TAU * freq / RATE as f32).cos();
    let (mut s1, mut s2) = (0.0f32, 0.0f32);
    for &v in x {
        let s0 = v + coeff * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    2.0 * (s1 * s1 + s2 * s2 - coeff * s1 * s2).max(0.0).sqrt() / x.len().max(1) as f32
}

/// Первый отсчёт, где слышна посылка 1 кГц (окна по 1 мс), или None.
pub fn find_onset(mono: &[f32]) -> Option<usize> {
    let win = (RATE / 1000) as usize;
    mono.chunks_exact(win).position(|w| tone_amplitude(w, FREQ) > 0.1).map(|i| i * win)
}

/// Задержка в миллисекундах. Запасы — те же, что у движка.
pub fn measure_cable(render_id: &str, capture_id: &str, render_reserve_ms: f64, capture_reserve_ms: f64) -> Result<f64, String> {
    let (out_stream, mut writer) = open_render(render_id, render_reserve_ms)?;
    let (in_stream, mut reader) = open_capture(Source::Device(capture_id.to_string()), capture_reserve_ms)?;
    let mut block = vec![0.0f32; BLOCK_SAMPLES];
    let mut heard = Vec::with_capacity(TOTAL_BLOCKS * BLOCK);
    let start = Instant::now();
    for b in 0..TOTAL_BLOCKS {
        // Пишем и читаем по блоку в такт, как движок: номера отсчётов совпадают по времени.
        let burst = (LEAD_BLOCKS..LEAD_BLOCKS + BURST_BLOCKS).contains(&b);
        for f in 0..BLOCK {
            let v = if burst { 0.5 * (((b - LEAD_BLOCKS) * BLOCK + f) as f32 * TAU * FREQ / RATE as f32).sin() } else { 0.0 };
            block[f * 2] = v;
            block[f * 2 + 1] = v;
        }
        writer.write(&block);
        reader.read(&mut block);
        heard.extend(block.iter().step_by(2).copied());
        let due = start + Duration::from_secs_f64((b + 1) as f64 * BLOCK as f64 / RATE as f64);
        if let Some(w) = due.checked_duration_since(Instant::now()) {
            std::thread::sleep(w);
        }
    }
    if let Some(e) = out_stream.failed().or_else(|| in_stream.failed()) {
        return Err(e);
    }
    let sent_at = LEAD_BLOCKS * BLOCK;
    match find_onset(&heard[sent_at..]) {
        Some(delay) => Ok(delay as f64 * 1000.0 / RATE as f64),
        None => Err("test tone was not heard on the virtual microphone".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_burst_after_silence() {
        let mut x = vec![0.0f32; 4800];
        x.extend((0..960).map(|i| 0.5 * (i as f32 * TAU * FREQ / RATE as f32).sin()));
        let onset = find_onset(&x).unwrap();
        assert!((4800..4800 + 96).contains(&onset), "{onset}");
        assert_eq!(find_onset(&[0.0; 4800]), None);
    }
}
