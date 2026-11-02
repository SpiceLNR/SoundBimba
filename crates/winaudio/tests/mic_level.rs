// SPDX-License-Identifier: AGPL-3.0-or-later
//! Насколько громко звучит голос с микрофона по умолчанию и что с ним делает компрессор.
//! Записывает микрофон (секунды — SB_LISTEN_SECONDS, по умолчанию 25), находит отрезки речи и
//! прогоняет запись через компрессор движка с разными настройками. Запуск вручную, пока человек
//! говорит: `cargo test -p winaudio --test mic_level -- --ignored --nocapture`

use std::time::{Duration, Instant};

use engine::compressor::State;
use engine::{AtomicF32, CompressorParams, BLOCK, BLOCK_SAMPLES, RATE};
use winaudio::devices;
use winaudio::streams::{open_capture, Source};

const WINDOW: usize = RATE as usize / 20; // 50 мс

fn db(x: f32) -> f32 {
    20.0 * x.max(1e-9).log10()
}

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|s| s * s).sum::<f32>() / x.len() as f32).sqrt()
}

fn percentile(mut v: Vec<f32>, p: f32) -> f32 {
    v.sort_by(f32::total_cmp);
    v[((v.len() - 1) as f32 * p) as usize]
}

/// Громкость речи: средний (медиана) и громкий (95 %) уровень окон речи и пик, дБ.
fn speech_stats(left: &[f32], speech: &[usize]) -> (f32, f32, f32) {
    let levels: Vec<f32> = speech.iter().map(|&w| db(rms(&left[w * WINDOW..(w + 1) * WINDOW]))).collect();
    let peak = speech
        .iter()
        .flat_map(|&w| left[w * WINDOW..(w + 1) * WINDOW].iter())
        .fold(0.0f32, |m, s| m.max(s.abs()));
    (percentile(levels.clone(), 0.5), percentile(levels, 0.95), db(peak))
}

#[test]
#[ignore = "нужен микрофон и человек, который говорит"]
fn voice_level_and_compressor() {
    let seconds: usize = std::env::var("SB_LISTEN_SECONDS").ok().and_then(|s| s.parse().ok()).unwrap_or(25);
    let devs = devices::list().expect("device list");
    let mic = devices::default_device(&devs, false).expect("нет микрофона по умолчанию");
    println!("микрофон: {}", mic.name);
    let (stream, mut reader) = open_capture(Source::Device(mic.id.clone()), 60.0).expect("open capture");
    let blocks = seconds * RATE as usize / BLOCK;
    let mut stereo: Vec<f32> = Vec::with_capacity(blocks * BLOCK_SAMPLES);
    let mut block = vec![0.0f32; BLOCK_SAMPLES];
    let start = Instant::now();
    for b in 0..blocks {
        reader.read(&mut block);
        stereo.extend_from_slice(&block);
        let due = start + Duration::from_secs_f64((b + 1) as f64 * BLOCK as f64 / RATE as f64);
        if let Some(w) = due.checked_duration_since(Instant::now()) {
            std::thread::sleep(w);
        }
    }
    drop(stream);
    stereo.drain(..RATE as usize * 2); // первая секунда — настройка буферов
    let left: Vec<f32> = stereo.iter().step_by(2).copied().collect();
    let windows = left.len() / WINDOW;
    let levels: Vec<f32> = (0..windows).map(|w| db(rms(&left[w * WINDOW..(w + 1) * WINDOW]))).collect();
    let floor = percentile(levels.clone(), 0.1);
    let speech: Vec<usize> = (0..windows).filter(|&w| levels[w] > floor + 15.0).collect();
    println!("фон (тишина): {floor:.1} дБ; окон с речью: {} из {windows}", speech.len());
    if speech.len() < 20 {
        println!("речи почти нет — говорите во время записи");
        return;
    }
    let (med, loud, peak) = speech_stats(&left, &speech);
    println!("{:<28} {:>9} {:>9} {:>7} {:>10}", "", "средн.", "громк.", "пик", "перегруз");
    println!("{:<28} {:>7.1}дБ {:>7.1}дБ {:>5.1}дБ", "без компрессора", med, loud, peak);

    // Наборы из app/ui/src/compressor.ts (порог, сила, атака, восстановление, усиление).
    let sets: [(&str, [f32; 5]); 3] = [
        ("Голос", [-24.0, 3.0, 5.0, 120.0, 12.0]),
        ("Громче тихое", [-40.0, 4.0, 3.0, 200.0, 20.0]),
        ("Сильно", [-30.0, 8.0, 1.0, 80.0, 22.0]),
    ];
    for (name, [t, r, a, rel, m]) in sets {
        let p = CompressorParams {
            threshold_db: AtomicF32::new(t),
            ratio: AtomicF32::new(r),
            attack_ms: AtomicF32::new(a),
            release_ms: AtomicF32::new(rel),
            makeup_db: AtomicF32::new(m),
        };
        let mut st = State::new(&p);
        let mut out = stereo.clone();
        for chunk in out.chunks_mut(BLOCK_SAMPLES) {
            st.process(chunk, &p);
        }
        let l: Vec<f32> = out.iter().step_by(2).copied().collect();
        let over = l.iter().filter(|s| s.abs() > 0.9).count() as f32 / l.len() as f32 * 100.0;
        let (m2, l2, p2) = speech_stats(&l, &speech);
        println!("{:<28} {:>7.1}дБ {:>7.1}дБ {:>5.1}дБ {:>8.2} %   (средн. {:+.1} дБ)", name, m2, l2, p2, over, m2 - med);
    }
}
