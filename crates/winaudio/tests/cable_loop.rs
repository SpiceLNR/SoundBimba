// SPDX-License-Identifier: AGPL-3.0-or-later
//! Проверка на настоящем железе: тон через виртуальный кабель (VB-CABLE) нашими потоками
//! вывода и захвата. Нужен установленный кабель, поэтому тест запускается вручную:
//! `cargo test -p winaudio --test cable_loop -- --ignored --nocapture`

use std::f32::consts::TAU;
use std::time::{Duration, Instant};

use engine::{BLOCK, BLOCK_SAMPLES, RATE};
use winaudio::devices;
use winaudio::streams::{open_capture, open_render, Source};

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
#[ignore = "нужен установленный VB-CABLE"]
fn tone_goes_through_the_cable() {
    let devs = devices::list().expect("device list");
    let speaker = devs
        .iter()
        .find(|d| d.render && d.cable.is_some() && d.name.starts_with("CABLE Input"))
        .expect("VB-CABLE «CABLE Input» не найден");
    let mic = devs
        .iter()
        .find(|d| !d.render && d.cable.is_some() && d.name.starts_with("CABLE Output"))
        .expect("VB-CABLE «CABLE Output» не найден");
    println!("динамик: {}\nмикрофон: {}", speaker.name, mic.name);

    // Те же запасы, что у приложения (app/src-tauri/src/audio.rs).
    let (out_stream, mut writer) = open_render(&speaker.id, 8.0).expect("open render");
    let (in_stream, mut reader) = open_capture(Source::Device(mic.id.clone()), 12.0).expect("open capture");
    println!("вывод: {:?}\nзахват: {:?}", out_stream.info, in_stream.info);

    // Сами играем роль движка: каждые 5 мс отдаём блок тона и забираем блок с микрофона.
    let seconds = 20.0;
    let blocks = (seconds * RATE as f64 / BLOCK as f64) as usize;
    let mut captured = Vec::with_capacity(blocks * BLOCK_SAMPLES);
    let mut block = vec![0.0f32; BLOCK_SAMPLES];
    let mut phase = 0usize;
    let start = Instant::now();
    for b in 0..blocks {
        for f in 0..BLOCK {
            let v = 0.5 * ((phase + f) as f32 * TAU * 440.0 / RATE as f32).sin();
            block[f * 2] = v;
            block[f * 2 + 1] = v;
        }
        phase += BLOCK;
        writer.write(&block);
        reader.read(&mut block);
        captured.extend_from_slice(&block);
        let due = start + Duration::from_secs_f64((b + 1) as f64 * BLOCK as f64 / RATE as f64);
        if let Some(wait) = due.checked_duration_since(Instant::now()) {
            std::thread::sleep(wait);
        }
    }
    assert!(out_stream.failed().is_none() && in_stream.failed().is_none());

    let left: Vec<f32> = captured.iter().step_by(2).copied().collect();
    let tail = &left[left.len() - 2 * RATE as usize..]; // последние 2 секунды
    // Подстройка под часы кабеля чуть сдвигает частоту (доли процента) — поэтому окна короткие
    // (20 мс), а частоту проверяем отдельно, по переходам через ноль.
    let window = RATE as usize / 50;
    let amps: Vec<f32> = tail.chunks_exact(window).map(|w| tone_amplitude(w, 440.0)).collect();
    let min_amp = amps.iter().copied().fold(f32::MAX, f32::min);
    let avg_amp = amps.iter().sum::<f32>() / amps.len() as f32;
    let crossings = tail.windows(2).filter(|w| (w[0] < 0.0) != (w[1] < 0.0)).count();
    let freq = crossings as f32 / 2.0 / (tail.len() as f32 / RATE as f32);
    let peak = tail.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    println!("амплитуда 440 Гц (окна 20 мс): средняя {avg_amp:.3}, наименьшая {min_amp:.3}; пик {peak:.3}");
    println!("частота ≈ {freq:.2} Гц");
    println!("счётчики (пропуски, переполнения, сбросы): вывод {:?}, захват {:?}", out_stream.stats.snapshot(), in_stream.stats.snapshot());
    // Щелчки: у чистой синусоиды вторая разность не больше A·(2πf/fs)² ≈ 0,0017. Любой разрыв
    // (потерянный или лишний кусочек звука) даёт скачок в десятки раз больше. Смотрим всё после
    // первой секунды (пока звук устанавливается).
    let steady = &left[RATE as usize..];
    let clicks: Vec<usize> = steady
        .windows(3)
        .enumerate()
        .filter(|(_, w)| (w[2] - 2.0 * w[1] + w[0]).abs() > 0.02)
        .map(|(i, _)| i)
        .collect();
    let mut events: Vec<usize> = Vec::new(); // соседние отсчёты одного щелчка — один щелчок
    for &i in &clicks {
        if events.last().is_none_or(|&l| i > l + 50) {
            events.push(i);
        }
    }
    let when: Vec<String> = events.iter().take(10).map(|&i| format!("{:.2} с", 1.0 + i as f64 / RATE as f64)).collect();
    println!("щелчков (разрывов звука) за {:.0} с: {} {when:?}", steady.len() as f64 / RATE as f64, events.len());
    assert!((avg_amp - 0.5).abs() < 0.05, "уровень не тот: {avg_amp}");
    assert!(min_amp > 0.4, "провал звука: {min_amp}");
    assert!((freq - 440.0).abs() < 440.0 * 0.01, "частота уплыла: {freq}");
}
