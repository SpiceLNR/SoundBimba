// SPDX-License-Identifier: AGPL-3.0-or-later
//! Автотесты движка без звукового железа.

use std::collections::HashMap;
use std::f32::consts::TAU;

use graph::{Edge, Node, NodeKind, PortKind, Project};
use serde_json::json;

use super::*;

struct Ports(HashMap<String, PortKey>);

impl PortResolver for Ports {
    fn port_for(&self, node: &Node) -> Option<PortKey> {
        self.0.get(&node.id).cloned()
    }
}

fn node(id: &str, kind: NodeKind, params: serde_json::Value) -> Node {
    Node { id: id.into(), kind, pos: [0.0, 0.0], params: params.as_object().cloned().unwrap_or_default() }
}

fn edge(from: &str, to: &str) -> Edge {
    Edge { from: from.into(), to: to.into(), kind: PortKind::Audio }
}

fn sine(frames: usize, freq: f32, amp: f32, rate: u32) -> Vec<f32> {
    (0..frames).flat_map(|i| {
        let v = amp * (i as f32 * TAU * freq / rate as f32).sin();
        [v, v]
    })
    .collect()
}

fn crossings(left: &[f32]) -> usize {
    left.windows(2).filter(|w| (w[0] < 0.0) != (w[1] < 0.0)).count()
}

// ---------- кольцевой буфер ----------

#[test]
fn ring_passes_samples_unchanged_at_same_rate() {
    // Записано ровно «цель + 3 кадра для интерполяции» — поправки скорости нет.
    let (mut w, mut r) = ring(4096, 48_000, 48_000, 297);
    let input: Vec<f32> = (0..300).flat_map(|i| [i as f32, -(i as f32)]).collect();
    w.write(&input);
    let mut out = vec![0.0; 200 * 2];
    r.read(&mut out);
    // Без поправки (заполнение около цели) и при равных частотах — те же отсчёты.
    assert_eq!(&out[..6], &[0.0, 0.0, 1.0, -1.0, 2.0, -2.0]);
}

#[test]
fn ring_waits_for_reserve_then_plays() {
    let (mut w, mut r) = ring(4096, 48_000, 48_000, 480);
    w.write(&vec![0.5; 100 * 2]);
    let mut out = vec![1.0; 64];
    r.read(&mut out);
    assert!(out.iter().all(|&s| s == 0.0), "мало данных — пока тишина");
    w.write(&vec![0.5; 600 * 2]);
    r.read(&mut out);
    assert!(out.iter().all(|&s| (s - 0.5).abs() < 1e-6));
}

#[test]
fn ring_converts_44100_to_48000_keeping_pitch() {
    let (mut w, mut r) = ring(48_000, 44_100, 48_000, 441);
    let input = sine(44_100, 1000.0, 0.5, 44_100);
    // Пишем и читаем порциями по 10 мс, как живые устройства.
    let mut left = Vec::new();
    let mut out = vec![0.0; 480 * 2];
    for chunk in input.chunks(441 * 2).take(80) {
        w.write(chunk);
        r.read(&mut out);
        left.extend(out.iter().step_by(2).copied());
    }
    // 40 000 кадров при 48 кГц ≈ 0,833 с → около 833 периодов (1666 переходов через ноль).
    let c = crossings(&left[1000..]);
    let expected = 2.0 * 1000.0 * (left.len() - 1000) as f64 / 48_000.0;
    assert!((c as f64 - expected).abs() < expected * 0.01, "{c} vs {expected}");
}

#[test]
fn ring_absorbs_clock_drift_without_overflow() {
    // Источник «спешит» на 0,1 %: за минуту без подстройки набежало бы 2880 лишних кадров.
    let (mut w, mut r) = ring(48_000, 48_048, 48_000, 960);
    let mut out = vec![0.0; BLOCK_SAMPLES];
    let chunk = vec![0.1f32; 481 * 2];
    let mut written = 0.0f64;
    let mut read = 0.0f64;
    for _ in 0..(60 * 200) {
        // 200 блоков по 5 мс в секунду; писатель отдаёт по 10 мс, но чуть быстрее.
        while written < read * 1.001 + 960.0 {
            w.write(&chunk);
            written += 481.0;
        }
        r.read(&mut out);
        read += BLOCK as f64;
    }
    let (under, over, skips) = r.stats().snapshot();
    assert_eq!((under, over, skips), (0, 0, 0));
    assert!((r.average_fill() - 960.0).abs() < 960.0, "заполнение держится около цели: {}", r.average_fill());
}

#[test]
fn ring_underrun_gives_silence_and_is_counted() {
    let (mut w, mut r) = ring(4096, 48_000, 48_000, 10);
    w.write(&[0.3; 20 * 2]);
    let mut out = vec![1.0; 100 * 2];
    r.read(&mut out);
    assert!(out[150..].iter().all(|&s| s == 0.0));
    assert_eq!(r.stats().snapshot().0, 1);
}

#[test]
fn writer_counts_overflow() {
    let (mut w, _r) = ring(16, 48_000, 48_000, 4);
    w.write(&vec![0.0; 1000]);
    assert_eq!(w.stats().snapshot().1, 1);
}

// ---------- граф ----------

struct Rig {
    graph: Graph,
    feeds: Vec<RingWriter>,
    taps: Vec<RingReader>,
    shared: HashMap<String, Arc<NodeShared>>,
}

/// Запас в тестовых кольцах: больше порции записи (блок), как и с настоящими устройствами.
const TEST_TARGET: usize = BLOCK * 2;

/// Собирает граф с портами-заглушками: в источники пишем сами, с выходов читаем сами.
fn rig(project: &Project, ports: &[(&str, PortKey)]) -> Rig {
    let resolver = Ports(ports.iter().map(|(id, k)| (id.to_string(), k.clone())).collect());
    let plan = plan(project, &resolver).unwrap();
    let mut feeds = Vec::new();
    let mut inputs = Vec::new();
    for _ in &plan.inputs {
        let (w, r) = ring(48_000, RATE, RATE, TEST_TARGET);
        feeds.push(w);
        inputs.push(r);
    }
    let mut outputs = Vec::new();
    let mut taps = Vec::new();
    for _ in &plan.outputs {
        let (w, r) = ring(48_000, RATE, RATE, TEST_TARGET);
        outputs.push(w);
        taps.push(r);
    }
    let mut shared = HashMap::new();
    let graph = Graph::compile(project, &resolver, &plan, inputs, outputs, &mut shared).unwrap();
    Rig { graph, feeds, taps, shared }
}

fn run_blocks(rig: &mut Rig, blocks: usize, feed: &[Vec<f32>]) -> Vec<Vec<f32>> {
    let mut outs: Vec<Vec<f32>> = rig.taps.iter().map(|_| Vec::new()).collect();
    for b in 0..blocks {
        for (w, data) in rig.feeds.iter_mut().zip(feed) {
            w.write(&data[b * BLOCK_SAMPLES..(b + 1) * BLOCK_SAMPLES]);
        }
        rig.graph.process();
        for (tap, out) in rig.taps.iter_mut().zip(outs.iter_mut()) {
            let mut buf = vec![0.0; BLOCK_SAMPLES];
            tap.read(&mut buf);
            out.extend_from_slice(&buf);
        }
    }
    outs
}

fn mixer_project(gain: f64, mute: bool) -> Project {
    Project {
        schema: 1,
        nodes: vec![
            node("app", NodeKind::AppSource, json!({ "exe": "x.exe" })),
            node("mic", NodeKind::MicInput, json!({})),
            node("vol", NodeKind::Volume, json!({ "gain": gain, "mute": mute })),
            node("out", NodeKind::VirtualOut, json!({})),
        ],
        edges: vec![edge("app", "vol"), edge("vol", "out"), edge("mic", "out")],
    }
}

fn ports() -> Vec<(&'static str, PortKey)> {
    vec![
        ("app", PortKey::Process(42)),
        ("mic", PortKey::Capture("mic-id".into())),
        ("out", PortKey::Render("cable-id".into())),
    ]
}

#[test]
fn inputs_are_summed_and_volume_applied() {
    let p = mixer_project(0.5, false);
    let mut r = rig(&p, &ports());
    let blocks = 40;
    let app = vec![0.4f32; blocks * BLOCK_SAMPLES];
    let mic = vec![0.1f32; blocks * BLOCK_SAMPLES];
    let out = run_blocks(&mut r, blocks, &[app, mic]);
    // app × 0,5 + mic = 0,2 + 0,1.
    let tail = &out[0][out[0].len() - BLOCK_SAMPLES..];
    assert!(tail.iter().all(|&s| (s - 0.3).abs() < 1e-5), "{:?}", &tail[..4]);
}

#[test]
fn mute_silences_only_that_branch() {
    let p = mixer_project(1.0, true);
    let mut r = rig(&p, &ports());
    let blocks = 40;
    let out = run_blocks(&mut r, blocks, &[vec![0.4; blocks * BLOCK_SAMPLES], vec![0.1; blocks * BLOCK_SAMPLES]]);
    let tail = &out[0][out[0].len() - BLOCK_SAMPLES..];
    assert!(tail.iter().all(|&s| (s - 0.1).abs() < 1e-5));
}

#[test]
fn gain_change_is_smooth() {
    let p = mixer_project(1.0, false);
    let mut r = rig(&p, &ports());
    let blocks = 10;
    let ones = vec![0.5f32; blocks * BLOCK_SAMPLES];
    let zeros = vec![0.0f32; blocks * BLOCK_SAMPLES];
    run_blocks(&mut r, 5, &[ones.clone(), zeros.clone()]);
    // Интерфейс меняет громкость с 100 % до 0 % — без скачка внутри блока.
    let mut q = p.clone();
    q.nodes[2].params.insert("gain".into(), json!(0.0));
    r.shared["vol"].apply_params(&q.nodes[2]);
    let out = run_blocks(&mut r, 3, &[ones, zeros]);
    let max_step = out[0].windows(3).map(|w| (w[2] - w[0]).abs()).fold(0.0f32, f32::max);
    assert!(max_step < 0.01, "скачок {max_step}");
}

#[test]
fn nodes_without_ports_are_silent_and_levels_are_reported() {
    let p = mixer_project(1.0, false);
    // У приложения нет порта (не запущено): играет только микрофон.
    let only_mic: Vec<_> = ports().into_iter().filter(|(id, _)| *id != "app").collect();
    let mut r = rig(&p, &only_mic);
    let blocks = 10;
    let out = run_blocks(&mut r, blocks, &[vec![0.25; blocks * BLOCK_SAMPLES]]);
    let tail = &out[0][out[0].len() - BLOCK_SAMPLES..];
    assert!(tail.iter().all(|&s| (s - 0.25).abs() < 1e-5));
    assert_eq!(r.shared["app"].take_peak(), 0.0);
    assert!((r.shared["mic"].take_peak() - 0.25).abs() < 1e-5);
    assert_eq!(r.shared["mic"].take_peak(), 0.0, "пик сбрасывается после чтения");
}

#[test]
fn shared_values_survive_recompile_and_unused_are_dropped() {
    let p = mixer_project(1.0, false);
    let mut r = rig(&p, &ports());
    let before = r.shared["vol"].clone();
    let resolver = Ports(ports().into_iter().map(|(id, k)| (id.to_string(), k)).collect());
    let mut q = p.clone();
    q.nodes.retain(|n| n.id != "mic");
    q.edges.retain(|e| e.from != "mic");
    let plan = plan(&q, &resolver).unwrap();
    let ins = plan.inputs.iter().map(|_| ring(64, RATE, RATE, 1).1).collect();
    let outs = plan.outputs.iter().map(|_| ring(64, RATE, RATE, 1).0).collect();
    Graph::compile(&q, &resolver, &plan, ins, outs, &mut r.shared).unwrap();
    assert!(Arc::ptr_eq(&before, &r.shared["vol"]));
    assert!(!r.shared.contains_key("mic"));
}

#[test]
fn two_nodes_on_one_device_share_a_port() {
    let mut p = mixer_project(1.0, false);
    p.nodes.push(node("out2", NodeKind::VirtualOut, json!({})));
    p.edges.push(edge("mic", "out2"));
    let mut list = ports();
    list.push(("out2", PortKey::Render("cable-id".into())));
    let resolver = Ports(list.into_iter().map(|(id, k)| (id.to_string(), k)).collect());
    assert_eq!(plan(&p, &resolver).unwrap().outputs.len(), 1);
}

#[test]
fn two_nodes_on_one_output_are_mixed_once_per_block() {
    let mut p = mixer_project(1.0, false);
    p.nodes.push(node("out2", NodeKind::VirtualOut, json!({})));
    p.edges.push(edge("mic", "out2"));
    let mut list = ports();
    list.push(("out2", PortKey::Render("cable-id".into())));
    let mut r = rig(&p, &list);
    let blocks = 40;
    let out = run_blocks(&mut r, blocks, &[vec![0.0; blocks * BLOCK_SAMPLES], vec![0.1; blocks * BLOCK_SAMPLES]]);
    // Ровно один блок на блок (время не растягивается), микрофон слышен дважды: 0,1 + 0,1.
    assert_eq!(out[0].len(), blocks * BLOCK_SAMPLES);
    assert_eq!(r.taps[0].stats().snapshot(), (0, 0, 0));
    let tail = &out[0][out[0].len() - BLOCK_SAMPLES..];
    assert!(tail.iter().all(|&s| (s - 0.2).abs() < 1e-5), "{:?}", &tail[..2]);
}

#[test]
fn soft_limiter_keeps_quiet_sound_and_tames_peaks() {
    let mut buf = vec![0.5, -0.89, 0.95, -1.5, 3.0, 1.0];
    let overloaded = super::soft_limit(&mut buf);
    assert!(overloaded);
    assert_eq!(&buf[..2], &[0.5, -0.89], "тихое не трогаем");
    assert!(buf[2] > 0.9 && buf[2] < 0.95);
    assert!(buf[3] > -1.0 && buf[3] < -0.9);
    assert!(buf[4] <= 1.0 && buf[4] > buf[2], "громче — ближе к максимуму, но не выше");
    let mut quiet = vec![0.3; 8];
    assert!(!super::soft_limit(&mut quiet));
}

#[test]
fn loud_mix_is_limited_and_counted() {
    // Два громких источника по 0,8 — в сумме 1,6: без ограничителя был бы жёсткий срез.
    let p = mixer_project(1.0, false);
    let mut r = rig(&p, &ports());
    let blocks = 40;
    let out = run_blocks(&mut r, blocks, &[vec![0.8; blocks * BLOCK_SAMPLES], vec![0.8; blocks * BLOCK_SAMPLES]]);
    let tail = &out[0][out[0].len() - BLOCK_SAMPLES..];
    assert!(tail.iter().all(|&s| s <= 1.0 && s > 0.9), "{:?}", &tail[..2]);
    assert!(r.taps[0].stats().overloads() > 0);
}
