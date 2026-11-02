// SPDX-License-Identifier: AGPL-3.0-or-later
//! Слушает «микрофон» виртуального кабеля (CABLE Output) 3 секунды и сообщает уровень и частоту.
//! Нужен для сквозной проверки: программа → SoundBimba → кабель. Запуск вручную:
//! `cargo test -p winaudio --test cable_listen -- --ignored --nocapture`

use std::time::{Duration, Instant};

use engine::{BLOCK, BLOCK_SAMPLES, RATE};
use winaudio::devices;
use winaudio::streams::{open_capture, Source};

#[test]
#[ignore = "нужен VB-CABLE и работающий SoundBimba"]
fn listen_to_cable_output() {
    let devs = devices::list().expect("device list");
    let mic = devs
        .iter()
        .find(|d| !d.render && d.cable.is_some() && d.name.starts_with("CABLE Output"))
        .expect("VB-CABLE «CABLE Output» не найден");
    let (stream, mut reader) = open_capture(Source::Device(mic.id.clone()), 15.0).expect("open capture");
    let blocks = 3 * RATE as usize / BLOCK;
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
    let tail = &left[left.len() / 3..];
    let peak = tail.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    let rms = (tail.iter().map(|s| s * s).sum::<f32>() / tail.len() as f32).sqrt();
    let crossings = tail.windows(2).filter(|w| (w[0] < 0.0) != (w[1] < 0.0)).count();
    let freq = crossings as f32 / 2.0 / (tail.len() as f32 / RATE as f32);
    println!("CABLE Output: пик {peak:.4}, амплитуда синуса ≈ {:.4}, частота ≈ {freq:.1} Гц", rms * std::f32::consts::SQRT_2);
    println!("счётчики захвата {:?}", stream.stats.snapshot());
    assert!(peak > 0.01, "на кабеле тишина");
}
