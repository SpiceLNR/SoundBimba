// SPDX-License-Identifier: AGPL-3.0-or-later
//! Управление звуком: держит движок и потоки устройств в соответствии с проектом.
//!
//! Работает в своём потоке (команды через канал), чтобы открытие устройств не тормозило окно.
//! - Изменились провода, узлы или устройства → движок и потоки перезапускаются (короткая пауза).
//! - Изменилась только громкость → новое значение уходит в движок без перезапуска.
//! - Раз в 2 секунды: не запустилась ли выбранная программа, не отключили ли устройство.
//!
//! Уровни и состояния узлов ~15 раз в секунду уходят в интерфейс событием `audio-state`.

use std::collections::HashMap;
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use engine::{Graph, NodeShared, PortKey, PortResolver, RATE};
use graph::{Node, NodeKind, Project};
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use winaudio::devices::{self, DeviceInfo};
use winaudio::streams::{self, Source, Stream};
use winaudio::{processes, runner};

use crate::applog;

/// Наименьший запас в кольцевых буферах, мс (часть общей задержки). Для устройств с крупными
/// порциями (10 мс) поток сам берёт больше — см. `winaudio::streams` (иначе треск).
pub const RENDER_RESERVE_MS: f64 = 8.0;
pub const CAPTURE_RESERVE_MS: f64 = 12.0;
const TICK: Duration = Duration::from_secs(2);
const RETRY_AFTER_FAILURE: Duration = Duration::from_secs(5);
const LEVELS_EVERY: Duration = Duration::from_millis(66);

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum NodeStatus {
    Ok,
    /// Звук выключен кнопкой.
    Off,
    AppNotChosen,
    AppNotRunning,
    DeviceMissing,
    NoCable,
    /// Устройство не открылось или отвалилось.
    Error,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PortReport {
    pub port: String,
    pub format: String,
    pub reserve_ms: f64,
    pub underruns: u32,
    pub overflows: u32,
    pub skips: u32,
    /// Блоков с перегрузом (сработал ограничитель) — только у выходов.
    pub overloads: u32,
    pub error: Option<String>,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub enabled: bool,
    pub running: bool,
    pub status: HashMap<String, NodeStatus>,
    pub ports: Vec<PortReport>,
    pub engine_late: u32,
    /// Последний замер «SoundBimba → виртуальный микрофон», мс.
    pub latency_ms: Option<f64>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct StateEvent<'a> {
    #[serde(flatten)]
    snapshot: &'a Snapshot,
    levels: HashMap<String, f32>,
}

/// Как узлы проекта связаны с устройствами и процессами прямо сейчас.
struct Resolved {
    ports: HashMap<String, PortKey>,
    status: HashMap<String, NodeStatus>,
}

impl PortResolver for Resolved {
    fn port_for(&self, node: &Node) -> Option<PortKey> {
        self.ports.get(&node.id).cloned()
    }
}

fn param<'a>(n: &'a Node, key: &str) -> Option<&'a str> {
    n.params.get(key).and_then(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty())
}

fn find_device<'a>(devs: &'a [DeviceInfo], wanted: Option<&str>, render: bool) -> Option<&'a DeviceInfo> {
    match wanted {
        None | Some("default") => devices::default_device(devs, render),
        Some(id) => devs.iter().find(|d| d.render == render && d.id == id),
    }
}

fn resolve(p: &Project) -> Resolved {
    let devs = devices::list().unwrap_or_default();
    let procs = processes::list();
    let mut r = Resolved { ports: HashMap::new(), status: HashMap::new() };
    for n in &p.nodes {
        let (port, status) = match n.kind {
            NodeKind::AppSource => match param(n, "exe") {
                None => (None, NodeStatus::AppNotChosen),
                Some(exe) => match processes::find_root(&procs, exe) {
                    Some(pid) => (Some(PortKey::Process(pid)), NodeStatus::Ok),
                    None => (None, NodeStatus::AppNotRunning),
                },
            },
            NodeKind::MicInput => match find_device(&devs, param(n, "device"), false) {
                Some(d) => (Some(PortKey::Capture(d.id.clone())), NodeStatus::Ok),
                None => (None, NodeStatus::DeviceMissing),
            },
            NodeKind::SpeakerOut => match find_device(&devs, param(n, "device"), true) {
                Some(d) => (Some(PortKey::Render(d.id.clone())), NodeStatus::Ok),
                None => (None, NodeStatus::DeviceMissing),
            },
            NodeKind::VirtualOut => match param(n, "device") {
                // «virtual-1» (или пусто) — основной виртуальный кабель, какой найдётся.
                None | Some("virtual-1") | Some("default") => match devices::primary_cable_render(&devs) {
                    Some(d) => (Some(PortKey::Render(d.id.clone())), NodeStatus::Ok),
                    None => (None, NodeStatus::NoCable),
                },
                Some(id) => match devs.iter().find(|d| d.render && d.id == id) {
                    Some(d) => (Some(PortKey::Render(d.id.clone())), NodeStatus::Ok),
                    None => (None, NodeStatus::DeviceMissing),
                },
            },
            NodeKind::SoundButton | NodeKind::Volume => (None, NodeStatus::Ok),
        };
        if let Some(port) = port {
            r.ports.insert(n.id.clone(), port);
        }
        r.status.insert(n.id.clone(), status);
    }
    r
}

/// Всё, что требует перезапуска движка. Положения узлов и громкость сюда не входят.
#[derive(PartialEq)]
struct Structure {
    nodes: Vec<(String, NodeKind)>,
    edges: Vec<(String, String)>,
    ports: Vec<(String, PortKey)>,
}

impl Structure {
    fn of(p: &Project, r: &Resolved) -> Structure {
        let mut ports: Vec<_> = r.ports.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        ports.sort_by(|a, b| a.0.cmp(&b.0));
        Structure {
            nodes: p.nodes.iter().map(|n| (n.id.clone(), n.kind)).collect(),
            edges: p.edges.iter().map(|e| (e.from.clone(), e.to.clone())).collect(),
            ports,
        }
    }
}

fn port_name(k: &PortKey) -> String {
    match k {
        PortKey::Process(pid) => format!("process {pid}"),
        PortKey::Capture(id) => format!("capture {id}"),
        PortKey::Render(id) => format!("render {id}"),
    }
}

struct Running {
    _runner: runner::Runner,
    late: Arc<std::sync::atomic::AtomicU32>,
    streams: Vec<(PortKey, Option<Stream>, Option<String>)>,
}

enum Cmd {
    Project(Box<Project>),
    Enabled(bool),
    Shutdown,
}

struct Controller {
    project: Option<Project>,
    enabled: bool,
    running: Option<Running>,
    structure: Option<Structure>,
    last_restart: Instant,
    shared: Arc<Mutex<HashMap<String, Arc<NodeShared>>>>,
    snapshot: Arc<Mutex<Snapshot>>,
    last_status: HashMap<String, NodeStatus>,
    last_glitches: HashMap<String, (u32, u32, u32)>,
    last_overloads: HashMap<String, u32>,
    last_late: u32,
}

impl Controller {
    fn stop(&mut self) {
        if self.running.take().is_some() {
            applog::info("audio stopped");
        }
        self.structure = None;
    }

    fn refresh(&mut self) {
        let Some(project) = self.project.clone() else { return };
        if !self.enabled {
            self.stop();
            let mut s = self.snapshot.lock().unwrap();
            s.enabled = false;
            s.running = false;
            s.status = project.nodes.iter().map(|n| (n.id.clone(), NodeStatus::Off)).collect();
            s.ports.clear();
            return;
        }
        let resolved = resolve(&project);
        let structure = Structure::of(&project, &resolved);
        let failed = self
            .running
            .as_ref()
            .is_some_and(|r| r.streams.iter().any(|(_, s, _)| s.as_ref().is_some_and(|s| s.failed().is_some())));
        let changed = self.structure.as_ref() != Some(&structure);
        let retry = failed && self.last_restart.elapsed() > RETRY_AFTER_FAILURE;
        if self.running.is_none() || changed || retry {
            let why = if self.running.is_none() { "start" } else if changed { "graph or devices changed" } else { "device failed, retrying" };
            applog::info(&format!("audio restart: {why}"));
            self.restart(&project, &resolved);
            self.structure = Some(structure);
        } else {
            // Тот же граф — только громкость.
            let shared = self.shared.lock().unwrap();
            for n in &project.nodes {
                if let Some(sh) = shared.get(&n.id) {
                    sh.apply_params(n);
                }
            }
        }
        self.publish(&project, &resolved);
    }

    fn restart(&mut self, project: &Project, resolved: &Resolved) {
        self.stop();
        self.last_glitches.clear();
        self.last_overloads.clear();
        self.last_late = 0;
        self.last_restart = Instant::now();
        let plan = match engine::plan(project, resolved) {
            Ok(p) => p,
            Err(e) => {
                applog::error(&format!("audio graph is invalid: {e}"));
                return;
            }
        };
        let mut streams_info = Vec::new();
        let mut inputs = Vec::new();
        for key in &plan.inputs {
            let source = match key {
                PortKey::Process(pid) => Source::Process(*pid),
                PortKey::Capture(id) => Source::Device(id.clone()),
                PortKey::Render(_) => unreachable!("render port in inputs"),
            };
            match streams::open_capture(source, CAPTURE_RESERVE_MS) {
                Ok((stream, reader)) => {
                    applog::info(&format!("opened {}: {:?}", port_name(key), stream.info));
                    inputs.push(reader);
                    streams_info.push((key.clone(), Some(stream), None));
                }
                Err(e) => {
                    applog::error(&format!("cannot open {}: {e}", port_name(key)));
                    // Порт молчит: читатель без писателя всегда отдаёт тишину.
                    inputs.push(engine::ring(1, RATE, RATE, 1).1);
                    streams_info.push((key.clone(), None, Some(e)));
                }
            }
        }
        let mut outputs = Vec::new();
        for key in &plan.outputs {
            let PortKey::Render(id) = key else { unreachable!("capture port in outputs") };
            match streams::open_render(id, RENDER_RESERVE_MS) {
                Ok((stream, writer)) => {
                    applog::info(&format!("opened {}: {:?}", port_name(key), stream.info));
                    outputs.push(writer);
                    streams_info.push((key.clone(), Some(stream), None));
                }
                Err(e) => {
                    applog::error(&format!("cannot open {}: {e}", port_name(key)));
                    outputs.push(engine::ring(1, RATE, RATE, 1).0);
                    streams_info.push((key.clone(), None, Some(e)));
                }
            }
        }
        let graph = {
            let mut shared = self.shared.lock().unwrap();
            match Graph::compile(project, resolved, &plan, inputs, outputs, &mut shared) {
                Ok(g) => g,
                Err(e) => {
                    applog::error(&format!("audio graph is invalid: {e}"));
                    return;
                }
            }
        };
        let runner = runner::start(graph);
        let late = runner.late.clone();
        applog::info(&format!(
            "audio started: {} inputs, {} outputs",
            plan.inputs.len(),
            plan.outputs.len()
        ));
        self.running = Some(Running { _runner: runner, late, streams: streams_info });
    }

    fn publish(&mut self, project: &Project, resolved: &Resolved) {
        let mut status = resolved.status.clone();
        let mut ports = Vec::new();
        if let Some(r) = &self.running {
            for (key, stream, open_error) in &r.streams {
                let error = open_error.clone().or_else(|| stream.as_ref().and_then(|s| s.failed()));
                if error.is_some() {
                    for (id, k) in &resolved.ports {
                        if k == key {
                            status.insert(id.clone(), NodeStatus::Error);
                        }
                    }
                }
                let (underruns, overflows, skips) = stream.as_ref().map(|s| s.stats.snapshot()).unwrap_or_default();
                let overloads = stream.as_ref().map(|s| s.stats.overloads()).unwrap_or_default();
                ports.push(PortReport {
                    port: port_name(key),
                    format: stream.as_ref().map(|s| s.info.format.clone()).unwrap_or_default(),
                    reserve_ms: stream.as_ref().map(|s| s.info.reserve_ms).unwrap_or_default(),
                    underruns,
                    overflows,
                    skips,
                    overloads,
                    error,
                });
            }
        }
        // Изменения состояний узлов — в журнал (для диагностики).
        for (id, st) in &status {
            if self.last_status.get(id) != Some(st) {
                applog::info(&format!("audio node {id}: {st:?}"));
            }
        }
        self.last_status = status.clone();
        // Сбои звука (пропуски, переполнения, сбросы, опоздания движка) — в журнал по приросту.
        for p in &ports {
            let now = (p.underruns, p.overflows, p.skips);
            let before = self.last_glitches.insert(p.port.clone(), now).unwrap_or_default();
            if now != before && (now.0 > before.0 || now.1 > before.1 || now.2 > before.2) {
                applog::info(&format!(
                    "audio glitches on {}: underruns +{}, overflows +{}, skips +{}",
                    p.port,
                    now.0.saturating_sub(before.0),
                    now.1.saturating_sub(before.1),
                    now.2.saturating_sub(before.2)
                ));
            }
        }
        for p in &ports {
            let before = self.last_overloads.insert(p.port.clone(), p.overloads).unwrap_or_default();
            if p.overloads > before {
                applog::info(&format!(
                    "audio overload on {}: {} block(s) louder than maximum, limiter engaged",
                    p.port,
                    p.overloads - before
                ));
            }
        }
        let late = self.running.as_ref().map(|r| r.late.load(std::sync::atomic::Ordering::Relaxed)).unwrap_or(0);
        if late > self.last_late {
            applog::info(&format!("audio engine was late {} time(s)", late - self.last_late));
        }
        self.last_late = late;
        let mut s = self.snapshot.lock().unwrap();
        s.enabled = true;
        s.running = self.running.is_some();
        s.status = status;
        s.ports = ports;
        s.engine_late = self.running.as_ref().map(|r| r.late.load(std::sync::atomic::Ordering::Relaxed)).unwrap_or(0);
        // Узлы, которых нет в проекте, из состояния убираем.
        s.status.retain(|id, _| project.nodes.iter().any(|n| &n.id == id));
    }
}

/// Ручка управления звуком для команд Tauri.
pub struct Audio {
    tx: Mutex<Sender<Cmd>>,
    snapshot: Arc<Mutex<Snapshot>>,
    control: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl Audio {
    pub fn start(app: AppHandle) -> Audio {
        let (tx, rx) = channel::<Cmd>();
        let snapshot = Arc::new(Mutex::new(Snapshot::default()));
        let shared: Arc<Mutex<HashMap<String, Arc<NodeShared>>>> = Arc::default();

        let mut c = Controller {
            project: None,
            enabled: false,
            running: None,
            structure: None,
            last_restart: Instant::now(),
            shared: shared.clone(),
            snapshot: snapshot.clone(),
            last_status: HashMap::new(),
            last_glitches: HashMap::new(),
            last_overloads: HashMap::new(),
            last_late: 0,
        };
        let control = std::thread::Builder::new()
            .name("sb-audio-control".into())
            .spawn(move || {
                if let Err(e) = winaudio::devices::ensure_com() {
                    applog::error(&format!("COM init failed in audio control: {e}"));
                }
                loop {
                    match rx.recv_timeout(TICK) {
                        Ok(Cmd::Project(p)) => c.project = Some(*p),
                        Ok(Cmd::Enabled(on)) => {
                            c.enabled = on;
                            applog::info(if on { "sound on" } else { "sound off" });
                        }
                        Ok(Cmd::Shutdown) => break,
                        Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                    c.refresh();
                }
                c.stop();
            })
            .expect("cannot start audio control thread");

        // Уровни и состояния — в интерфейс.
        let (snap, sh) = (snapshot.clone(), shared);
        std::thread::Builder::new()
            .name("sb-audio-levels".into())
            .spawn(move || loop {
                std::thread::sleep(LEVELS_EVERY);
                let levels: HashMap<String, f32> =
                    sh.lock().unwrap().iter().map(|(id, s)| (id.clone(), s.take_peak())).collect();
                let snapshot = snap.lock().unwrap().clone();
                if app.emit("audio-state", StateEvent { snapshot: &snapshot, levels }).is_err() {
                    break;
                }
            })
            .expect("cannot start audio levels thread");

        Audio { tx: Mutex::new(tx), snapshot, control: Mutex::new(Some(control)) }
    }

    /// Остановить звук и дождаться, пока устройства закроются (при выходе из программы).
    pub fn shutdown(&self) {
        let _ = self.tx.lock().unwrap().send(Cmd::Shutdown);
        if let Some(t) = self.control.lock().unwrap().take() {
            let _ = t.join();
        }
    }

    pub fn set_project(&self, p: Project) {
        let _ = self.tx.lock().unwrap().send(Cmd::Project(Box::new(p)));
    }

    pub fn set_enabled(&self, on: bool) {
        let _ = self.tx.lock().unwrap().send(Cmd::Enabled(on));
    }

    pub fn set_latency(&self, ms: f64) {
        self.snapshot.lock().unwrap().latency_ms = Some(ms);
    }

    pub fn snapshot(&self) -> Snapshot {
        self.snapshot.lock().unwrap().clone()
    }
}
