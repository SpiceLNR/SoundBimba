// SPDX-License-Identifier: AGPL-3.0-or-later
//! Автотесты движка без звукового железа.

use std::collections::HashMap;
use std::f32::consts::TAU;

use graph::{Edge, Node, NodeKind, Project};
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
    Edge::audio(from, to)
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
    let graph = Graph::compile(project, &resolver, &plan, inputs, outputs, &mut HashMap::new(), &mut shared).unwrap();
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
fn compressor_in_the_graph_evens_out_loud_and_quiet() {
    // Микрофон → компрессор (порог −20 дБ, сила 4, усиление +6 дБ) → выход.
    let p = Project {
        schema: 1,
        nodes: vec![
            node("mic", NodeKind::MicInput, json!({})),
            node("comp", NodeKind::Compressor, json!({ "threshold": -20.0, "ratio": 4.0, "attack": 1.0, "release": 50.0, "makeup": 6.0 })),
            node("out", NodeKind::VirtualOut, json!({})),
        ],
        edges: vec![edge("mic", "comp"), edge("comp", "out")],
    };
    let ports = [("mic", PortKey::Capture("mic-id".into())), ("out", PortKey::Render("cable-id".into()))];
    let level = |amp: f32| {
        let mut r = rig(&p, &ports);
        let blocks = 200;
        let out = run_blocks(&mut r, blocks, &[vec![amp; blocks * BLOCK_SAMPLES]]);
        let tail = &out[0][out[0].len() - BLOCK_SAMPLES..];
        (20.0 * tail[0].abs().log10(), r.shared["comp"].take_reduction())
    };
    // Тихое (−40 дБ) — только +6 дБ; громкое (−4 дБ) — превышение 16 дБ сжато до 4, плюс 6.
    let (quiet, quiet_red) = level(0.01);
    let (loud, loud_red) = level(10f32.powf(-4.0 / 20.0));
    assert!((quiet - (-34.0)).abs() < 0.1, "{quiet}");
    assert!((loud - (-10.0)).abs() < 0.2, "{loud}");
    assert!(quiet_red < 0.01 && (loud_red - 12.0).abs() < 0.3, "{quiet_red} {loud_red}");
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
    Graph::compile(&q, &resolver, &plan, ins, outs, &mut HashMap::new(), &mut r.shared).unwrap();
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
fn limiter_leaves_quiet_sound_untouched() {
    let input: Vec<f32> = sine(RATE as usize / 2, 440.0, 0.9, RATE);
    let mut buf = input.clone();
    let mut gain = 1.0;
    for chunk in buf.chunks_mut(BLOCK_SAMPLES) {
        assert!(!super::limit(chunk, &mut gain));
    }
    assert_eq!(buf, input, "тише потолка — ни одного изменённого отсчёта");
}

#[test]
fn limiter_turns_down_volume_without_distorting_the_wave() {
    // Тон в 1,5 раза громче максимума (так бывает, когда громкий трек складывается с голосом).
    let input = sine(RATE as usize, 440.0, 1.5, RATE);
    let mut buf = input.clone();
    let mut gain = 1.0;
    let mut overloaded = false;
    for chunk in buf.chunks_mut(BLOCK_SAMPLES) {
        overloaded |= super::limit(chunk, &mut gain);
    }
    assert!(overloaded, "перегруз замечен");
    assert!(buf.iter().all(|s| s.abs() <= LIMIT_CEILING + 1e-6), "выше потолка ничего нет");
    // После первых 10 мс — та же синусоида, только тише: форма волны не искажена.
    let k = LIMIT_CEILING / 1.5;
    let worst = buf.iter().zip(&input).skip(RATE as usize / 100 * CHANNELS).map(|(o, i)| (o - i * k).abs()).fold(0.0f32, f32::max);
    assert!(worst < 0.01, "искажение {worst}");
}

#[test]
fn limiter_lets_go_after_the_peak() {
    let mut gain = 1.0;
    let mut loud = vec![1.96f32; BLOCK_SAMPLES];
    super::limit(&mut loud, &mut gain);
    assert!((gain - 0.5).abs() < 1e-3, "пик вдвое громче потолка — громкость вдвое ниже");
    // Полсекунды тихого звука — громкость вернулась почти полностью.
    for _ in 0..(RATE as usize / 2 / BLOCK) {
        let mut quiet = vec![0.1f32; BLOCK_SAMPLES];
        super::limit(&mut quiet, &mut gain);
    }
    assert!(gain > 0.99, "{gain}");
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

// ---------- кнопки ----------

/// Кнопка → выход. Звук: `head` кадров в памяти (уровень 0,5), всего `total` кадров;
/// если total > head — остальное приходит через подкачку, которой управляет сам тест.
/// Подкачка, которой управляет тест: куда класть звук и что просил движок.
type TestFeed = Option<(rtrb::Producer<f32>, Arc<TailControl>)>;

fn button_rig_with(head: usize, total: u64) -> (Graph, RingReader, HashMap<String, Arc<NodeShared>>, TestFeed) {
    let p = Project {
        schema: 1,
        nodes: vec![node("btn", NodeKind::SoundButton, json!({ "file": "x.wav" })), node("out", NodeKind::VirtualOut, json!({}))],
        edges: vec![edge("btn", "out")],
    };
    let resolver = Ports([("out".to_string(), PortKey::Render("cable".into()))].into_iter().collect());
    let (feed, tail) = if total > head as u64 {
        let (prod, cons) = rtrb::RingBuffer::new(48_000 * CHANNELS);
        let ctl = Arc::new(TailControl::default());
        (Some((prod, ctl.clone())), Some(TailFeed { cons, ctl }))
    } else {
        (None, None)
    };
    let src = ButtonSource { head: vec![0.5f32; head * CHANNELS].into(), total_frames: total, tail };
    let plan = plan(&p, &resolver).unwrap();
    // Запас = ровно 4 блока минус 3 кадра: после 4 блоков читатель стартует, ничего не отбросив.
    let (w, r) = ring(48_000, RATE, RATE, BLOCK * 4 - 3);
    let mut shared = HashMap::new();
    let mut buttons = HashMap::from([("btn".to_string(), src)]);
    let g = Graph::compile(&p, &resolver, &plan, vec![], vec![w], &mut buttons, &mut shared).unwrap();
    (g, r, shared, feed)
}

fn button_rig(frames: usize) -> (Graph, RingReader, HashMap<String, Arc<NodeShared>>) {
    let (g, r, s, _) = button_rig_with(frames, frames as u64);
    (g, r, s)
}

fn drain(r: &mut RingReader) -> Vec<f32> {
    let mut all = Vec::new();
    let mut buf = vec![0.0; BLOCK_SAMPLES];
    loop {
        let before = r.stats().snapshot().0;
        r.read(&mut buf);
        if r.stats().snapshot().0 != before {
            break;
        }
        all.extend_from_slice(&buf);
    }
    all
}

#[test]
fn button_is_silent_until_pressed_then_plays_to_the_end() {
    let (mut g, _r, shared) = button_rig(BLOCK * 3 + 10);
    let btn = shared["btn"].clone();
    g.process();
    assert_eq!(btn.playback(), None, "до нажатия молчит");
    assert_eq!(btn.button_state().unwrap().total_frames, (BLOCK * 3 + 10) as u64, "длина известна заранее");
    btn.press();
    g.process();
    let p1 = btn.playback().expect("играет");
    assert!((p1 - BLOCK as f32 / (BLOCK * 3 + 10) as f32).abs() < 1e-4);
    for _ in 0..3 {
        g.process();
    }
    assert_eq!(btn.playback(), None, "доиграла и остановилась сама");
}

#[test]
fn second_press_pauses_with_a_short_fade_and_third_resumes() {
    let (mut g, mut r, shared) = button_rig(RATE as usize);
    let btn = shared["btn"].clone();
    btn.press();
    g.process();
    g.process();
    btn.press();
    g.process();
    g.process();
    assert_eq!(btn.playback(), None, "повторное нажатие — пауза");
    let paused_at = btn.button_state().unwrap().pos_frames;
    assert!(paused_at > (BLOCK * 2) as u64, "позиция сохранена: {paused_at}");
    btn.press();
    g.process();
    let s = btn.button_state().unwrap();
    assert!(s.playing && s.pos_frames == paused_at + BLOCK as u64, "дальше с того же места");
    let out = drain(&mut r);
    let left: Vec<f32> = out.iter().step_by(2).copied().collect();
    assert!(left.iter().any(|&s| (s - 0.5).abs() < 1e-6));
    // Вступление, пауза и продолжение — плавно: без скачка больше 1/200 уровня за отсчёт.
    let max_step = left.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
    assert!(max_step <= 0.5 / STOP_FADE as f32 + 1e-4, "скачок {max_step}");
}

#[test]
fn end_of_sound_resets_the_time_scale() {
    let (mut g, _r, shared) = button_rig(BLOCK * 2);
    let btn = shared["btn"].clone();
    btn.press();
    for _ in 0..3 {
        g.process();
    }
    let s = btn.button_state().unwrap();
    assert!(!s.playing && s.pos_frames == 0, "доиграла — стоп и шкала на начало");
    btn.press();
    g.process();
    assert!(btn.playback().unwrap() > 0.0, "нажатие после конца — снова с начала");
}

#[test]
fn press_after_end_starts_again_from_the_beginning() {
    let (mut g, _r, shared) = button_rig(BLOCK * 2);
    let btn = shared["btn"].clone();
    btn.press();
    for _ in 0..3 {
        g.process();
    }
    assert_eq!(btn.playback(), None, "доиграла");
    btn.press();
    g.process();
    assert!((btn.playback().unwrap() - 0.5).abs() < 1e-6);
}

#[test]
fn seek_within_memory_starts_playing_there() {
    let (mut g, _r, shared) = button_rig(BLOCK * 10);
    let btn = shared["btn"].clone();
    btn.seek(0.5); // кнопка стояла — перемотка запускает с середины
    g.process();
    let s = btn.button_state().unwrap();
    assert!(s.playing);
    assert_eq!(s.pos_frames, (BLOCK * 5 + BLOCK) as u64);
}

#[test]
fn long_sound_continues_from_disk_after_the_head() {
    use std::sync::atomic::Ordering::Acquire;
    let head = BLOCK * 2;
    let (mut g, mut r, shared, feed) = button_rig_with(head, (BLOCK * 6) as u64);
    let (mut prod, ctl) = feed.unwrap();
    let btn = shared["btn"].clone();
    btn.press();
    g.process();
    // Движок попросил подкачку с конца «головы».
    assert_eq!(ctl.want_gen.load(Acquire), 1);
    assert_eq!(ctl.want_pos.load(Acquire), head as u64);
    // Подкачка: сначала старый мусор, потом метка и звук 0,25.
    for _ in 0..10 {
        prod.push(9.0).unwrap();
    }
    prod.push(f32::NAN).unwrap();
    prod.push(1.0).unwrap();
    for _ in 0..BLOCK * 4 * CHANNELS {
        prod.push(0.25).unwrap();
    }
    // (Звука с диска меньше 0,1 с запаса — это весь остаток трека, ждать больше нечего.)
    // Пики на выходе по блокам: 1-й блок уже сыгран (память), дальше — ещё блок памяти и диск.
    let out = shared["out"].clone();
    let mut peaks = vec![out.take_peak()];
    for _ in 0..5 {
        g.process();
        peaks.push(out.take_peak());
    }
    assert_eq!(btn.playback(), None, "доиграла весь звук");
    assert!(peaks[0] > 0.49 && peaks[0] <= 0.5, "начало — из памяти (с плавным вступлением)");
    assert_eq!(peaks[1], 0.5);
    assert!(peaks[2..].iter().all(|&p| p > 0.24 && p <= 0.25), "дальше — с диска: {peaks:?}");
    let _ = &mut r;
}

#[test]
fn head_to_disk_seam_has_no_dip() {
    use std::sync::atomic::Ordering::Acquire;
    // Звук 0,5 и в памяти, и с диска; подкачка готова заранее — на стыке громкость не проседает
    // (раньше там было «плавное вступление» с нуля — щелчок на 2-й секунде длинных звуков).
    let head = BLOCK * 2;
    let (mut g, mut r, shared, feed) = button_rig_with(head, (BLOCK * 8) as u64);
    let (mut prod, ctl) = feed.unwrap();
    let btn = shared["btn"].clone();
    btn.press();
    g.process();
    prod.push(f32::NAN).unwrap();
    prod.push(ctl.want_gen.load(Acquire) as f32).unwrap();
    for _ in 0..BLOCK * 6 * CHANNELS {
        prod.push(0.5).unwrap();
    }
    // Первый блок уже обработан; дальше — блок движка и блок с выхода, как в жизни.
    let mut left = Vec::new();
    let mut buf = vec![0.0; BLOCK_SAMPLES];
    for _ in 0..14 {
        g.process();
        r.read(&mut buf);
        left.extend(buf.iter().step_by(2).copied());
    }
    // Выход отдаёт звук после запаса кольца; ищем начало и смотрим 6 блоков после вступления.
    let start = left.iter().position(|&s| s > 0.0).expect("звук пошёл");
    // После вступления первых 5 мс — ровно 0,5 до конца звука, в том числе на стыке.
    let body = &left[start + STOP_FADE..start + BLOCK * 7];
    let min = body.iter().copied().fold(f32::MAX, f32::min);
    assert!(min > 0.499, "провал на стыке до {min}");
}

#[test]
fn seek_past_the_head_asks_disk_and_waits_silently() {
    use std::sync::atomic::Ordering::Acquire;
    let (mut g, _r, shared, feed) = button_rig_with(BLOCK, (BLOCK * 100) as u64);
    let (_prod, ctl) = feed.unwrap();
    let btn = shared["btn"].clone();
    btn.seek(0.5);
    g.process();
    assert_eq!(ctl.want_pos.load(Acquire), (BLOCK * 50) as u64);
    // Подкачка ещё не ответила — позиция стоит, кнопка считается играющей.
    let s = btn.button_state().unwrap();
    assert!(s.playing);
    assert_eq!(s.pos_frames, (BLOCK * 50) as u64);
}

#[test]
fn after_seek_waits_for_a_reserve_instead_of_stuttering() {
    use std::sync::atomic::Ordering::Acquire;
    let (mut g, _r, shared, feed) = button_rig_with(BLOCK, (RATE * 60) as u64);
    let (mut prod, ctl) = feed.unwrap();
    let btn = shared["btn"].clone();
    btn.seek(0.5);
    g.process();
    let gen = ctl.want_gen.load(Acquire);
    // Подкачка ответила меткой и пока только 10 мс звука — играть рано.
    prod.push(f32::NAN).unwrap();
    prod.push(gen as f32).unwrap();
    for _ in 0..(RATE as usize / 100) * CHANNELS {
        prod.push(0.3).unwrap();
    }
    let out = shared["out"].clone();
    out.take_peak();
    g.process();
    assert_eq!(out.take_peak(), 0.0, "ждём запаса 0,1 с");
    // Подкачка догнала — звук пошёл.
    for _ in 0..(RATE as usize / 5) * CHANNELS {
        prod.push(0.3).unwrap();
    }
    g.process();
    g.process();
    assert!(out.take_peak() > 0.29);
}

#[test]
fn track_ends_when_the_file_ends_even_if_the_header_promised_more() {
    use std::sync::atomic::Ordering::{Acquire, Release};
    // По заголовку — 4 блока, но с диска придёт на 2 кадра меньше (так бывает у MP3).
    let head = BLOCK;
    let (mut g, _r, shared, feed) = button_rig_with(head, (BLOCK * 4) as u64);
    let (mut prod, ctl) = feed.unwrap();
    let btn = shared["btn"].clone();
    btn.press();
    g.process();
    let gen = ctl.want_gen.load(Acquire);
    prod.push(f32::NAN).unwrap();
    prod.push(gen as f32).unwrap();
    for _ in 0..(BLOCK * 3 - 2) * CHANNELS {
        prod.push(0.2).unwrap();
    }
    ctl.eof_gen.store(gen, Release);
    for _ in 0..5 {
        g.process();
    }
    let s = btn.button_state().unwrap();
    assert!(!s.playing && s.pos_frames == 0, "кончился файл — стоп и шкала на начало: {s:?}");
}
