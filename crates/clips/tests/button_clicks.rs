// SPDX-License-Identifier: AGPL-3.0-or-later
//! Щелчки в звуке кнопки без звуковых устройств: файл с чистым тоном 44,1 кГц (как у большинства
//! MP3) → кнопка в движке (начало из памяти, остальное — подкачкой с диска, перевод в 48 кГц) →
//! ищем разрывы по каждому отсчёту. Идёт в реальном времени (~6 с), поэтому запуск вручную:
//! `cargo test -p clips --test button_clicks -- --ignored --nocapture`

use std::collections::HashMap;
use std::f32::consts::TAU;
use std::io::Write;
use std::time::{Duration, Instant};

use engine::{Graph, PortKey, PortResolver, BLOCK, BLOCK_SAMPLES, CHANNELS, RATE};
use graph::{Edge, Node, NodeKind, PortKind, Project};

struct OnlyOut;

impl PortResolver for OnlyOut {
    fn port_for(&self, node: &Node) -> Option<PortKey> {
        (node.kind == NodeKind::VirtualOut).then(|| PortKey::Render("x".into()))
    }
}

fn write_wav(path: &std::path::Path, rate: u32, samples: &[i16]) {
    let data_len = (samples.len() * 2) as u32;
    let mut f = std::fs::File::create(path).unwrap();
    for part in [
        b"RIFF".as_slice(),
        &(36 + data_len).to_le_bytes(),
        b"WAVEfmt ",
        &16u32.to_le_bytes(),
        &1u16.to_le_bytes(),
        &2u16.to_le_bytes(),
    ] {
        f.write_all(part).unwrap();
    }
    for part in [rate.to_le_bytes(), (rate * 4).to_le_bytes()] {
        f.write_all(&part).unwrap();
    }
    f.write_all(&4u16.to_le_bytes()).unwrap();
    f.write_all(&16u16.to_le_bytes()).unwrap();
    f.write_all(b"data").unwrap();
    f.write_all(&data_len.to_le_bytes()).unwrap();
    for s in samples {
        f.write_all(&s.to_le_bytes()).unwrap();
    }
}

/// Номера отсчётов (левый канал), где вторая разность больше, чем бывает у синуса.
fn clicks(left: &[f32], freq: f32, amp: f32) -> Vec<usize> {
    let limit = amp * (TAU * freq / RATE as f32).powi(2) * 8.0 + 0.01;
    let mut out: Vec<usize> = Vec::new();
    for (i, w) in left.windows(3).enumerate() {
        if (w[2] - 2.0 * w[1] + w[0]).abs() > limit && out.last().is_none_or(|&l| i > l + 50) {
            out.push(i);
        }
    }
    out
}

#[test]
#[ignore = "идёт в реальном времени ~6 с"]
fn button_sound_has_no_clicks() {
    // 6 с тона 440 Гц на 44,1 кГц, уровень 0,5.
    let src_rate = 44_100u32;
    let n = src_rate as usize * 6;
    let pcm: Vec<i16> = (0..n)
        .flat_map(|i| {
            let v = (0.5 * (TAU * 440.0 * i as f32 / src_rate as f32).sin() * 32767.0) as i16;
            [v, v]
        })
        .collect();
    let path = std::env::temp_dir().join(format!("sb-button-clicks-{}.wav", std::process::id()));
    write_wav(&path, src_rate, &pcm);

    let (src, streamer) = clips::open(&path).expect("open");
    let node = |id: &str, kind| Node {
        id: id.into(),
        kind,
        pos: [0.0, 0.0],
        params: Default::default(),
    };
    let project = Project {
        schema: 1,
        nodes: vec![
            node("b", NodeKind::SoundButton),
            node("o", NodeKind::VirtualOut),
        ],
        edges: vec![Edge {
            from: "b".into(),
            to: "o".into(),
            kind: PortKind::Audio,
        }],
    };
    let plan = engine::plan(&project, &OnlyOut).unwrap();
    let (w, mut r) = engine::ring(RATE as usize * 10, RATE, RATE, BLOCK * 2);
    let mut shared = HashMap::new();
    let mut buttons = HashMap::from([("b".to_string(), src)]);
    let mut g = Graph::compile(
        &project,
        &OnlyOut,
        &plan,
        vec![],
        vec![w],
        &mut buttons,
        &mut shared,
    )
    .unwrap();
    shared["b"].press();

    // Как поток движка: блок раз в 5 мс.
    let blocks = 6 * RATE as usize / BLOCK;
    let mut left = Vec::with_capacity(blocks * BLOCK);
    let mut buf = vec![0.0f32; BLOCK_SAMPLES];
    let start = Instant::now();
    for b in 0..blocks {
        g.process();
        r.read(&mut buf);
        left.extend(buf.iter().step_by(CHANNELS).copied());
        let due = start + Duration::from_secs_f64((b + 1) as f64 * BLOCK as f64 / RATE as f64);
        if let Some(wait) = due.checked_duration_since(Instant::now()) {
            std::thread::sleep(wait);
        }
    }
    drop(streamer);
    let _ = std::fs::remove_file(&path);

    // Начало (вступление 5 мс) и конец (затухание) пропускаем: смотрим 0,1…5,5 с звука.
    let first = left
        .iter()
        .position(|s| s.abs() > 0.01)
        .expect("звук не пошёл");
    let body = &left[first + RATE as usize / 10..first + RATE as usize * 11 / 2];
    let found = clicks(body, 440.0, 0.5);
    let peak = body.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    let at: Vec<String> = found
        .iter()
        .take(10)
        .map(|i| format!("{:.3} с", 0.1 + *i as f64 / RATE as f64))
        .collect();
    println!("пик {peak:.3}; щелчков: {} {at:?}", found.len());
    assert!(found.is_empty(), "щелчки в звуке кнопки: {at:?}");
}
