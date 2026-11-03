// SPDX-License-Identifier: AGPL-3.0-or-later
//! Опыт для этапа 7 (с разрешения автора 2026-11-03): направить звук отдельной программы на другое
//! устройство (недокументированный механизм `winaudio::routing`) и проверить, что копия через
//! захват процесса остаётся. Меняется настройка Windows только для тестовой программы (этот же
//! файл вторым процессом) и сразу сбрасывается на «по умолчанию».
//! Запуск вручную: `cargo test -p winaudio --test redirect_original -- --ignored --nocapture --test-threads 1`

use std::f32::consts::TAU;
use std::time::{Duration, Instant};

use engine::{RingReader, BLOCK, BLOCK_SAMPLES, RATE};
use winaudio::streams::{open_capture, open_render, Source};
use winaudio::{devices, routing};

const CHILD_ENV: &str = "SB_REDIRECT_CHILD";

/// Тестовая программа: сразу открывает устройство вывода «по умолчанию» (как обычная программа)
/// и 5 с играет тихий тон; затем закрывает его и открывает «по умолчанию» заново — как после
/// перезапуска — и играет ещё 5 с.
#[test]
#[ignore = "запускается опытом redirect_then_capture"]
fn redirect_child() {
    if std::env::var(CHILD_ENV).is_err() {
        return;
    }
    let mut phase = 0usize;
    for round in 0..2 {
        let devs = devices::list().expect("devices");
        let default = devices::default_device(&devs, true).expect("нет устройства по умолчанию");
        eprintln!("child: открытие {round}: «по умолчанию» для меня — {}", default.name);
        let (_out, mut writer) = open_render(&default.id, 20.0).expect("render");
        let mut block = vec![0.0f32; BLOCK_SAMPLES];
        let start = Instant::now();
        for b in 0..(5 * RATE as usize / BLOCK) {
            for f in 0..BLOCK {
                let v = 0.05 * (TAU * 440.0 * (phase + f) as f32 / RATE as f32).sin();
                block[f * 2] = v;
                block[f * 2 + 1] = v;
            }
            phase += BLOCK;
            writer.write(&block);
            let due = start + Duration::from_secs_f64((b + 1) as f64 * BLOCK as f64 / RATE as f64);
            if let Some(w) = due.checked_duration_since(Instant::now()) {
                std::thread::sleep(w);
            }
        }
    }
}

fn tone(x: &[f32]) -> f32 {
    let coeff = 2.0 * (TAU * 440.0 / RATE as f32).cos();
    let (mut s1, mut s2) = (0.0f32, 0.0f32);
    for &v in x {
        let s0 = v + coeff * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    2.0 * (s1 * s1 + s2 * s2 - coeff * s1 * s2).max(0.0).sqrt() / x.len() as f32
}

fn listen(readers: &mut [&mut RingReader], seconds: f32) -> Vec<f32> {
    let blocks = (seconds * RATE as f32 / BLOCK as f32) as usize;
    let mut got: Vec<Vec<f32>> = readers.iter().map(|_| Vec::new()).collect();
    let mut buf = vec![0.0f32; BLOCK_SAMPLES];
    let start = Instant::now();
    for i in 0..blocks {
        for (r, g) in readers.iter_mut().zip(got.iter_mut()) {
            r.read(&mut buf);
            g.extend(buf.iter().step_by(2).copied());
        }
        let due = start + Duration::from_secs_f64((i + 1) as f64 * BLOCK as f64 / RATE as f64);
        if let Some(w) = due.checked_duration_since(Instant::now()) {
            std::thread::sleep(w);
        }
    }
    got.iter().map(|g| tone(&g[g.len() - 2 * RATE as usize..])).collect()
}

#[test]
#[ignore = "нужен VB-CABLE; меняет настройку Windows для тестовой программы и сбрасывает её"]
fn redirect_then_capture() {
    let devs = devices::list().expect("devices");
    let cable = devices::primary_cable_render(&devs).expect("нет VB-CABLE");
    let cable_mic = devices::primary_cable_capture(&devs).expect("нет CABLE Output").id.clone();
    let default = devices::default_device(&devs, true).expect("нет устройства по умолчанию");
    println!("устройство по умолчанию: {}; перенаправляем на: {}", default.name, cable.name);

    // Сначала только чтение — у нашей собственной программы настройки быть не должно.
    println!("чтение настройки для себя: {:?}", routing::output_of(std::process::id()));
    if std::env::var("SB_READ_ONLY").is_ok() {
        return; // первый запуск — только проверить, что чтение работает
    }

    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["redirect_child", "--exact", "--ignored", "--nocapture"])
        .env(CHILD_ENV, "1")
        .stdout(std::process::Stdio::null())
        .spawn()
        .expect("запуск тестовой программы");
    let pid = child.id();
    let (_s1, mut on_cable) = open_capture(Source::Device(cable_mic), 60.0).expect("микрофон кабеля");
    let (_s2, mut on_default) = open_capture(Source::Loopback(default.id.clone()), 60.0).expect("loopback");
    std::thread::sleep(Duration::from_millis(1200));
    let (_s3, mut copy) = open_capture(Source::Process(pid), 60.0).expect("захват процесса");
    println!("пока играет, у тестовой программы: {:?}", routing::output_of(pid));
    let set = routing::set_output_of(pid, Some(&cable.id));
    println!("перенаправление: {set:?}; теперь: {:?}", routing::output_of(pid));
    let a = listen(&mut [&mut on_cable, &mut on_default, &mut copy], 3.2);
    println!("А. звук, который уже играл:    на кабеле {:.3}, на «по умолчанию» {:.3}, копия {:.3}", a[0], a[1], a[2]);
    std::thread::sleep(Duration::from_millis(1200));
    let b = listen(&mut [&mut on_cable, &mut on_default, &mut copy], 3.0);
    println!("Б. после повторного открытия:  на кабеле {:.3}, на «по умолчанию» {:.3}, копия {:.3}", b[0], b[1], b[2]);

    let reset = routing::set_output_of(pid, None);
    println!("сброс: {reset:?}; теперь: {:?}", routing::output_of(pid));
    let _ = child.kill();
    let _ = child.wait();
}
