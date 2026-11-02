// SPDX-License-Identifier: AGPL-3.0-or-later
//! Проверка подкачки на настоящем файле: играем в реальном времени, на 2-й секунде прыгаем
//! на 5-ю минуту и считаем, сколько раз звуку не хватило данных. Запуск вручную:
//! `set SB_TRACK=путь\к\треку.mp3` и `cargo test -p clips --test seek_real_file -- --ignored --nocapture`

use std::sync::atomic::Ordering::Release;
use std::time::{Duration, Instant};

use engine::{BLOCK, CHANNELS, RATE};

#[test]
#[ignore = "нужен настоящий длинный трек в SB_TRACK"]
fn seek_on_a_real_track_without_gaps() {
    let Ok(path) = std::env::var("SB_TRACK") else { return };
    let t0 = Instant::now();
    let (mut src, streamer) = clips::open(std::path::Path::new(&path)).expect("open");
    println!("открыт за {:?}; длина {:.1} с", t0.elapsed(), src.total_frames as f64 / RATE as f64);
    let tail = src.tail.as_mut().expect("длинный трек");
    let head_frames = src.head.len() / CHANNELS;
    // На 5-ю минуту, а если трек короче — на 70 % длины.
    let target = (300 * RATE as u64).min(src.total_frames * 7 / 10);
    println!("прыжок на {:.1} с", target as f64 / RATE as f64);

    // Как движок: сначала с конца «головы», через 2 с — прыжок на 300 с.
    let mut gen = 1u32;
    tail.ctl.want_pos.store(head_frames as u64, Release);
    tail.ctl.want_gen.store(gen, Release);
    let mut synced = false;
    let mut buffering = true;
    let mut starved_blocks = 0;
    let mut wait_after_seek: Option<Instant> = None;
    let start = Instant::now();
    let blocks = 7 * RATE as usize / BLOCK;
    for b in 0..blocks {
        if b == 2 * RATE as usize / BLOCK {
            gen += 1;
            synced = false;
            buffering = true;
            tail.ctl.want_pos.store(target, Release);
            tail.ctl.want_gen.store(gen, Release);
            wait_after_seek = Some(Instant::now());
        }
        while !synced {
            let Ok(v) = tail.cons.pop() else { break };
            if v.is_nan() {
                if let Ok(g) = tail.cons.pop() {
                    synced = g as u32 == gen;
                }
            }
        }
        // Как движок: после метки ждём запаса 0,1 с и только потом играем.
        if synced && buffering && tail.cons.slots() >= (RATE as usize / 10) * CHANNELS {
            buffering = false;
        }
        if synced && !buffering {
            if let Some(w) = wait_after_seek.take() {
                println!("после перемотки звук пошёл через {:?}", w.elapsed());
            }
            if tail.cons.slots() < BLOCK * CHANNELS {
                starved_blocks += 1;
            }
            for _ in 0..(BLOCK * CHANNELS).min(tail.cons.slots()) {
                let _ = tail.cons.pop();
            }
        }
        let due = start + Duration::from_secs_f64((b + 1) as f64 * BLOCK as f64 / RATE as f64);
        if let Some(w) = due.checked_duration_since(Instant::now()) {
            std::thread::sleep(w);
        }
    }
    drop(streamer);
    println!("блоков без данных (после того как звук пошёл): {starved_blocks}");
    assert_eq!(starved_blocks, 0);
}
