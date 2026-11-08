// SPDX-License-Identifier: AGPL-3.0-or-later
//! Проверка на железе, как в программе: кнопка с чистым тоном → морфер → виртуальный кабель
//! (и, по желанию, ещё наушники через «Громкость»). Слушаем «микрофон» кабеля или сами наушники
//! (режим loopback: в наушниках будет слышен тихий тон) и ищем щелчки по каждому отсчёту. Запуск вручную (нужен VB-CABLE):
//! `cargo test -p winaudio --test morpher_outputs -- --ignored --nocapture --test-threads 1`

use std::collections::HashMap;
use std::f32::consts::TAU;
use std::sync::Arc;
use std::time::{Duration, Instant};

use engine::{ButtonSource, Graph, PortKey, PortResolver, BLOCK, BLOCK_SAMPLES, RATE};
use graph::{Edge, Node, NodeKind, Project};
use winaudio::devices;
use winaudio::streams::{open_capture, open_render, Source};

struct Ports(HashMap<String, PortKey>);

impl PortResolver for Ports {
    fn port_for(&self, node: &Node) -> Option<PortKey> {
        self.0.get(&node.id).cloned()
    }
}

fn node(id: &str, kind: NodeKind, params: serde_json::Value) -> Node {
    Node { id: id.into(), kind, pos: [0.0, 0.0], params: params.as_object().cloned().unwrap_or_default() }
}

fn edge(a: &str, b: &str) -> Edge {
    Edge::audio(a, b)
}

/// Щелчки в левом канале: вторая разность больше, чем бывает у синуса такой частоты и уровня.
fn clicks(left: &[f32], freq: f32, amp: f32) -> Vec<f32> {
    let limit = amp * (TAU * freq / RATE as f32).powi(2) * 8.0 + 0.01;
    let mut events = Vec::new();
    let mut last = 0usize;
    for (i, w) in left.windows(3).enumerate() {
        if (w[2] - 2.0 * w[1] + w[0]).abs() > limit && (events.is_empty() || i > last + 50) {
            events.push(i as f32 / RATE as f32);
            last = i;
        }
    }
    events
}

/// `phones_gain`: None — без наушников; Some(g) — ещё и в наушники с этой громкостью.
/// `listen_phones`: слушать наушники (loopback), а не кабель.
fn run(phones_gain: Option<f64>, listen_phones: bool, morph: serde_json::Value, seconds: f32) -> (usize, Vec<f32>) {
    let devs = devices::list().expect("devices");
    let cable = devices::primary_cable_render(&devs).expect("нет VB-CABLE");
    let cable_mic = devices::primary_cable_capture(&devs).expect("нет CABLE Output");
    let phones = devices::default_device(&devs, true).expect("нет наушников по умолчанию");

    let mut nodes = vec![
        node("btn", NodeKind::SoundButton, serde_json::json!({})),
        node("morph", NodeKind::VoiceMorpher, morph),
        node("virt", NodeKind::VirtualOut, serde_json::json!({})),
    ];
    let mut edges = vec![edge("btn", "morph"), edge("morph", "virt")];
    let mut ports = HashMap::from([("virt".to_string(), PortKey::Render(cable.id.clone()))]);
    if let Some(gain) = phones_gain {
        nodes.push(node("vol", NodeKind::Volume, serde_json::json!({ "gain": gain })));
        nodes.push(node("phones", NodeKind::SpeakerOut, serde_json::json!({})));
        edges.push(edge("morph", "vol"));
        edges.push(edge("vol", "phones"));
        ports.insert("phones".into(), PortKey::Render(phones.id.clone()));
    }
    let project = Project { schema: 1, nodes, edges };
    let resolver = Ports(ports);
    let plan = engine::plan(&project, &resolver).unwrap();
    // Те же запасы, что у приложения.
    let mut streams = Vec::new();
    let mut outputs = Vec::new();
    for key in &plan.outputs {
        let PortKey::Render(id) = key else { unreachable!() };
        let (s, w) = open_render(id, 8.0).expect("render");
        streams.push(s);
        outputs.push(w);
    }
    // Тон 440 Гц на всё время проверки — целиком в памяти кнопки.
    let frames = (RATE as f32 * (seconds + 2.0)) as usize;
    let tone: Vec<f32> = (0..frames)
        .flat_map(|i| {
            let v = 0.3 * (TAU * 440.0 * i as f32 / RATE as f32).sin();
            [v, v]
        })
        .collect();
    let mut buttons = HashMap::from([(
        "btn".to_string(),
        ButtonSource { head: Arc::from(tone.into_boxed_slice()), total_frames: frames as u64, tail: None },
    )]);
    let mut shared = HashMap::new();
    let graph = Graph::compile(&project, &resolver, &plan, vec![], outputs, &mut buttons, &mut shared).unwrap();
    shared["btn"].press();
    let runner = winaudio::runner::start(graph);

    let source = if listen_phones { Source::Loopback(phones.id.clone()) } else { Source::Device(cable_mic.id.clone()) };
    let (listen, mut reader) = open_capture(source, 60.0).expect("capture");
    let blocks = (seconds * RATE as f32 / BLOCK as f32) as usize;
    let mut left = Vec::with_capacity(blocks * BLOCK);
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
    let gaps: u32 = streams.iter().map(|s| s.stats.device_gaps()).sum::<u32>();
    let under: u32 = streams.iter().map(|s| s.stats.snapshot().0).sum::<u32>() + listen.stats.snapshot().0;
    drop(runner);
    drop(streams);
    let steady = &left[RATE as usize..];
    println!("  пропуски {under}, пустой буфер устройства {gaps}");
    let amp = if listen_phones { 0.3 * phones_gain.unwrap_or(1.0) as f32 } else { 0.3 };
    (steady.len(), clicks(steady, 440.0, amp))
}

#[test]
#[ignore = "нужен VB-CABLE"]
fn morpher_with_one_and_two_outputs() {
    let seconds = 20.0;
    for (label, phones, listen_phones, morph) in [
        ("обычный голос, только кабель; слушаем кабель", None, false, serde_json::json!({})),
        ("обычный голос, кабель + наушники 0 %; слушаем кабель", Some(0.0), false, serde_json::json!({})),
        ("обычный голос, кабель + наушники 15 %; слушаем наушники", Some(0.15), true, serde_json::json!({})),
        ("робот, кабель + наушники 15 %; слушаем наушники", Some(0.15), true, serde_json::json!({ "robot": 1.0 })),
    ] {
        println!("{label}:");
        let (_, events) = run(phones, listen_phones, morph, seconds);
        let shown: Vec<String> = events.iter().take(8).map(|t| format!("{:.2} с", t + 1.0)).collect();
        println!("  щелчков: {} {shown:?}", events.len());
    }
}

/// Для сравнения: тон прямо на наушники нашим потоком вывода, без движка и морфера.
#[test]
#[ignore = "нужны наушники; в них будет слышен тихий тон"]
fn tone_straight_to_headphones() {
    let devs = devices::list().expect("devices");
    // SB_DEVICE — часть названия устройства вывода (по умолчанию — устройство Windows по умолчанию).
    let phones = match std::env::var("SB_DEVICE") {
        Ok(name) => devs.iter().find(|d| d.render && d.name.contains(&name)).expect("нет такого устройства"),
        Err(_) => devices::default_device(&devs, true).expect("нет наушников по умолчанию"),
    };
    println!("устройство: {}", phones.name);
    let (out, mut writer) = open_render(&phones.id, 8.0).expect("render");
    println!("вывод: {:?}", out.info);
    let (listen, mut reader) = open_capture(Source::Loopback(phones.id.clone()), 60.0).expect("loopback");
    let seconds = 20.0;
    let blocks = (seconds * RATE as f32 / BLOCK as f32) as usize;
    let mut left = Vec::with_capacity(blocks * BLOCK);
    let mut block = vec![0.0f32; BLOCK_SAMPLES];
    let mut phase = 0usize;
    let amp = 0.045;
    let start = Instant::now();
    for b in 0..blocks {
        for f in 0..BLOCK {
            let v = amp * (TAU * 440.0 * (phase + f) as f32 / RATE as f32).sin();
            block[f * 2] = v;
            block[f * 2 + 1] = v;
        }
        phase += BLOCK;
        writer.write(&block);
        reader.read(&mut block);
        left.extend(block.iter().step_by(2).copied());
        let due = start + Duration::from_secs_f64((b + 1) as f64 * BLOCK as f64 / RATE as f64);
        if let Some(w) = due.checked_duration_since(Instant::now()) {
            std::thread::sleep(w);
        }
    }
    println!("пропуски вывод {:?}, loopback {:?}, пустой буфер {}", out.stats.snapshot(), listen.stats.snapshot(), out.stats.device_gaps());
    let events = clicks(&left[RATE as usize..], 440.0, amp);
    let shown: Vec<String> = events.iter().take(8).map(|t| format!("{:.2} с", t + 1.0)).collect();
    println!("щелчков: {} {shown:?}", events.len());
}
