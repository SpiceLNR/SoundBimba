// SPDX-License-Identifier: AGPL-3.0-or-later
//! Сколько звука на самом деле отдаёт подкачка по сравнению с длиной из заголовка файла:
//! если меньше — движок ждал бы конца, который не наступит. Запуск вручную:
//! `set SB_TRACK=путь\к\треку.mp3` и `cargo test -p clips --test real_track_end -- --ignored --nocapture`

use std::sync::atomic::Ordering::Release;
use std::time::{Duration, Instant};

use engine::{Graph, PortKey, PortResolver, CHANNELS, RATE};
use graph::{Edge, Node, NodeKind, PortKind, Project};
use std::collections::HashMap;

#[test]
#[ignore = "нужен настоящий трек в SB_TRACK"]
fn streamed_length_matches_header_length() {
    let Ok(path) = std::env::var("SB_TRACK") else { return };
    let (mut src, streamer) = clips::open(std::path::Path::new(&path)).expect("open");
    let head = (src.head.len() / CHANNELS) as u64;
    let total = src.total_frames;
    let tail = src.tail.as_mut().expect("длинный трек");
    // Просим последние 10 секунд и считаем, сколько придёт.
    let from = total.saturating_sub(10 * RATE as u64).max(head);
    tail.ctl.want_pos.store(from, Release);
    tail.ctl.want_gen.store(1, Release);
    let mut got = 0u64;
    let mut idle_since = Instant::now();
    while idle_since.elapsed() < Duration::from_millis(800) {
        let mut any = false;
        while let Ok(v) = tail.cons.pop() {
            if v.is_nan() {
                let _ = tail.cons.pop();
                continue;
            }
            got += 1;
            any = true;
        }
        if any {
            idle_since = Instant::now();
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let frames = got / CHANNELS as u64;
    drop(streamer);
    println!(
        "по заголовку до конца {} кадров, пришло {} (разница {} кадров = {:.1} мс)",
        total - from,
        frames,
        (total - from) as i64 - frames as i64,
        ((total - from) as f64 - frames as f64) * 1000.0 / RATE as f64
    );
}

struct OnlyOut;

impl PortResolver for OnlyOut {
    fn port_for(&self, node: &Node) -> Option<PortKey> {
        (node.kind == NodeKind::VirtualOut).then(|| PortKey::Render("x".into()))
    }
}

/// Вся цепочка, как в программе: кнопка в движке + подкачка настоящего файла. Перемотка почти
/// в конец — кнопка должна доиграть, остановиться и сбросить шкалу на начало.
#[test]
#[ignore = "нужен настоящий трек в SB_TRACK"]
fn real_track_resets_after_the_end() {
    let Ok(path) = std::env::var("SB_TRACK") else { return };
    let (src, streamer) = clips::open(std::path::Path::new(&path)).expect("open");
    let node = |id: &str, kind| Node { id: id.into(), kind, pos: [0.0, 0.0], params: Default::default() };
    let project = Project {
        schema: 1,
        nodes: vec![node("b", NodeKind::SoundButton), node("o", NodeKind::VirtualOut)],
        edges: vec![Edge { from: "b".into(), to: "o".into(), kind: PortKind::Audio }],
    };
    let plan = engine::plan(&project, &OnlyOut).unwrap();
    let (w, _r) = engine::ring(RATE as usize * 4, RATE, RATE, 1);
    let mut shared = HashMap::new();
    let mut buttons = HashMap::from([("b".to_string(), src)]);
    let mut g = Graph::compile(&project, &OnlyOut, &plan, vec![], vec![w], &mut buttons, &mut shared).unwrap();
    let btn = shared["b"].clone();
    btn.seek(0.995);
    let start = Instant::now();
    let mut reset = false;
    while start.elapsed() < Duration::from_secs(5) {
        g.process();
        let s = btn.button_state().unwrap();
        if !s.playing && s.pos_frames == 0 && start.elapsed() > Duration::from_millis(50) {
            reset = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    drop(streamer);
    println!("остановилась и сбросилась: {reset} за {:?}", start.elapsed());
    assert!(reset);
}
