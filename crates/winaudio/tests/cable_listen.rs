// SPDX-License-Identifier: AGPL-3.0-or-later
//! Слушает «микрофон» виртуального кабеля (CABLE Output) и проверяет качество чистого тона:
//! уровень, частоту, провалы и щелчки. Нужен для сквозной проверки: программа → SoundBimba →
//! кабель. Запуск вручную (секунды — переменная SB_LISTEN_SECONDS, по умолчанию 3):
//! `cargo test -p winaudio --test cable_listen -- --ignored --nocapture`

use std::f32::consts::TAU;
use std::time::{Duration, Instant};

use engine::{BLOCK, BLOCK_SAMPLES, RATE};
use winaudio::devices;
use winaudio::streams::{open_capture, Source};

fn tone_amplitude(x: &[f32], freq: f32) -> f32 {
    let coeff = 2.0 * (TAU * freq / RATE as f32).cos();
    let (mut s1, mut s2) = (0.0f32, 0.0f32);
    for &v in x {
        let s0 = v + coeff * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    2.0 * (s1 * s1 + s2 * s2 - coeff * s1 * s2).max(0.0).sqrt() / x.len() as f32
}

#[test]
#[ignore = "нужен VB-CABLE и работающий SoundBimba"]
fn listen_to_cable_output() {
    let seconds: usize = std::env::var("SB_LISTEN_SECONDS").ok().and_then(|s| s.parse().ok()).unwrap_or(3);
    let devs = devices::list().expect("device list");
    let mic = devices::primary_cable_capture(&devs).expect("VB-CABLE «CABLE Output» не найден");
    // Большой запас: этот тест засыпает обычным таймером Windows (погрешность до 15 мс) —
    // его собственные пропуски не должны выглядеть как сбои SoundBimba.
    let (stream, mut reader) = open_capture(Source::Device(mic.id.clone()), 60.0).expect("open capture");
    let blocks = seconds * RATE as usize / BLOCK;
    let mut left: Vec<f32> = Vec::with_capacity(blocks * BLOCK);
    let mut block = vec![0.0f32; BLOCK_SAMPLES];
    let start = Instant::now();
    for b in 0..blocks {
        reader.read(&mut block);
        left.extend(block.iter().step_by(2).copied());
        let due = start + Duration::from_secs_f64((b + 1) as f64 * BLOCK as f64 / RATE as f64);
        if let Some(w) = due.checked_duration_since(Instant::now()) {
            std::thread::sleep(w);
        }
    }
    // Первую секунду пропускаем (настройка буферов).
    let tail = &left[RATE as usize..];
    let peak = tail.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    let crossings = tail.windows(2).filter(|w| (w[0] < 0.0) != (w[1] < 0.0)).count();
    let freq = crossings as f32 / 2.0 / (tail.len() as f32 / RATE as f32);
    // Провалы: окна по 5 мс, где тон заметно тише обычного.
    let win = RATE as usize / 200;
    let amps: Vec<f32> = tail.chunks_exact(win).map(|w| tone_amplitude(w, freq.max(1.0))).collect();
    let mut sorted = amps.clone();
    sorted.sort_by(f32::total_cmp);
    let median = sorted[sorted.len() / 2];
    let dips = amps.iter().filter(|&&a| a < median * 0.8).count();
    // Щелчки: скачок между соседними отсчётами больше, чем бывает у синуса этой частоты и уровня.
    let max_slope = median * TAU * freq / RATE as f32;
    let clicks = tail.windows(2).filter(|w| (w[1] - w[0]).abs() > max_slope * 1.8 + 0.002).count();
    println!(
        "CABLE Output: пик {peak:.4}, тон {median:.4}, частота ≈ {freq:.1} Гц; провалов (окна 5 мс): {dips} из {}; щелчков: {clicks}",
        amps.len()
    );
    println!("счётчики захвата {:?}", stream.stats.snapshot());
    assert!(peak > 0.01, "на кабеле тишина");
}
