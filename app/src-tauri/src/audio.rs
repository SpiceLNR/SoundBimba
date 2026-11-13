// SPDX-License-Identifier: AGPL-3.0-or-later
//! Управление звуком: держит движок и потоки устройств в соответствии с проектом.
//!
//! Работает в своём потоке (команды через канал), чтобы открытие устройств не тормозило окно.
//! - Изменились провода, узлы или устройства → движок и потоки перезапускаются (короткая пауза).
//! - Изменилась только громкость → новое значение уходит в движок без перезапуска.
//! - Раз в 2 секунды: не запустилась ли выбранная программа, не отключили ли устройство.
//! - «Оригинал звука» программ (этап 7, `routes`): выставляется, пока звук включён, и
//!   возвращается, когда выбор снят, звук выключен или SoundBimba закрывается.
//!
//! Уровни и состояния узлов ~15 раз в секунду уходят в интерфейс событием `audio-state`.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use engine::{ButtonSource, Graph, NodeShared, PortKey, PortResolver, RATE};
use graph::{Node, NodeKind, Project};
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use winaudio::devices::{self, DeviceInfo};
use winaudio::streams::{self, Source, Stream};
use winaudio::{processes, runner};

use crate::applog;
use crate::routes::{self, Journal, RouteState, WindowsRoutes};

/// Наименьший запас в кольцевых буферах, мс (часть общей задержки). Для устройств с крупными
/// порциями (10 мс) поток сам берёт больше — см. `winaudio::streams` (иначе треск).
pub const RENDER_RESERVE_MS: f64 = 8.0;
pub const CAPTURE_RESERVE_MS: f64 = 12.0;
const TICK: Duration = Duration::from_secs(2);
const RETRY_AFTER_FAILURE: Duration = Duration::from_secs(5);
const LEVELS_EVERY: Duration = Duration::from_millis(66);
/// Устойчивый вывод: буфер устройства, мс (см. `RenderMode::Steady`). 40 мс — на «Virtual Desktop
/// Audio» (VR-шлем) провалов 0, при 60 мс щёлкал кабель (замер 2026-11-12).
const STEADY_BUFFER_MS: u32 = 40;

/// Устройство Virtual Desktop (звук по сети в VR-шлем): в быстром режиме у него трещит.
fn is_virtual_desktop(device_name: &str) -> bool {
    device_name.to_lowercase().contains("virtual desktop")
}

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
    /// Кнопка: файл не выбран.
    FileNotChosen,
    /// Кнопка: файла нет или нет доступа.
    FileMissing,
    /// Кнопка: формат не поддерживается или файл повреждён.
    FileUnsupported,
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
    /// Провалы на стороне Windows (потеря данных при захвате, пустой буфер вывода).
    pub device_gaps: u32,
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
    /// «Оригинал звука» узлов «Звук приложения», где выбрано перенаправление.
    pub routes: HashMap<String, RouteState>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct StateEvent<'a> {
    #[serde(flatten)]
    snapshot: &'a Snapshot,
    levels: HashMap<String, f32>,
    /// Кнопки со звуком: играет ли, позиция и длина (секунды) — для кругового индикатора и времени.
    buttons: HashMap<String, ButtonEvent>,
    /// Компрессоры: наибольшее сжатие с прошлого события, дБ — для индикатора.
    reductions: HashMap<String, f32>,
    /// Шумоподавление: вероятность голоса 0…1 с прошлого события — для индикатора.
    voices: HashMap<String, f32>,
}

#[derive(Serialize, Clone)]
struct ButtonEvent {
    playing: bool,
    pos: f64,
    len: f64,
}

/// Как узлы проекта связаны с устройствами, процессами и файлами прямо сейчас.
struct Resolved {
    ports: HashMap<String, PortKey>,
    /// Выходы Virtual Desktop (id устройств).
    vd_outputs: BTreeSet<String>,
    /// Кнопки с годным файлом: id узла → путь.
    clips: HashMap<String, String>,
    status: HashMap<String, NodeStatus>,
}

impl PortResolver for Resolved {
    fn port_for(&self, node: &Node) -> Option<PortKey> {
        self.ports.get(&node.id).cloned()
    }
}

/// Проверенные файлы кнопок (по пути). «Нет файла» не запоминаем — файл может появиться;
/// «не звук» запоминаем, чтобы не проверять плохой файл каждые 2 секунды.
type ClipCache = HashMap<String, Result<(), clips::ClipError>>;

fn clip_status(e: &clips::ClipError) -> NodeStatus {
    match e {
        clips::ClipError::Missing(_) => NodeStatus::FileMissing,
        clips::ClipError::Unsupported(_) => NodeStatus::FileUnsupported,
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

/// Последний удачно прочитанный список устройств. Не прочитался (Windows на миг не ответила) —
/// берём его: иначе все устройства, и кабель тоже, «пропадают», и звук перезапускается.
static LAST_DEVICES: std::sync::Mutex<Vec<DeviceInfo>> = std::sync::Mutex::new(Vec::new());
/// Какие виртуальные кабели видны сейчас (для журнала: когда кабель пропал и появился).
static LAST_CABLES: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

pub fn current_devices() -> Vec<DeviceInfo> {
    let mut last = LAST_DEVICES.lock().unwrap_or_else(|e| e.into_inner());
    match devices::list() {
        Ok(devs) if !devs.is_empty() => *last = devs,
        Ok(_) => applog::info("audio devices: Windows returned an empty list, keeping the previous one"),
        Err(e) => applog::error(&format!("audio devices: cannot list ({}), keeping the previous list", e.message())),
    }
    let cables: Vec<String> = last.iter().filter(|d| d.cable.is_some()).map(|d| format!("{}{}", if d.render { "out " } else { "in " }, d.name)).collect();
    let cables = if cables.is_empty() { "none".to_string() } else { cables.join(", ") };
    let mut seen = LAST_CABLES.lock().unwrap_or_else(|e| e.into_inner());
    if *seen != cables {
        applog::info(&format!("virtual cables now: {cables}"));
        *seen = cables;
    }
    last.clone()
}

fn resolve(p: &Project, cache: &mut ClipCache) -> Resolved {
    let devs = current_devices();
    let procs = processes::list();
    let mut r = Resolved { ports: HashMap::new(), vd_outputs: BTreeSet::new(), clips: HashMap::new(), status: HashMap::new() };
    r.vd_outputs = devs.iter().filter(|d| d.render && is_virtual_desktop(&d.name)).map(|d| d.id.clone()).collect();
    for n in &p.nodes {
        if n.kind == NodeKind::SoundButton {
            let status = match param(n, "file") {
                None => NodeStatus::FileNotChosen,
                Some(path) => {
                    let checked = match cache.get(path) {
                        Some(res) => res.clone(),
                        None => {
                            let res = clips::probe(std::path::Path::new(path));
                            // «Нет файла» не запоминаем и не пишем в журнал каждые 2 с — смену
                            // состояния узла журнал и так записывает.
                            if !matches!(res, Err(clips::ClipError::Missing(_))) {
                                if let Err(e) = &res {
                                    applog::error(&format!("button {}: {e}", n.id));
                                }
                                cache.insert(path.to_string(), res.clone());
                            }
                            res
                        }
                    };
                    match checked {
                        Ok(()) => {
                            r.clips.insert(n.id.clone(), path.to_string());
                            NodeStatus::Ok
                        }
                        Err(e) => clip_status(&e),
                    }
                }
            };
            r.status.insert(n.id.clone(), status);
            continue;
        }
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
            NodeKind::SoundButton
            | NodeKind::Volume
            | NodeKind::Compressor
            | NodeKind::VoiceMorpher
            | NodeKind::Denoise
            | NodeKind::Chip
            | NodeKind::ChipIn
            | NodeKind::ChipOut => (None, NodeStatus::Ok),
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
    clips: Vec<(String, String)>,
}

impl Structure {
    fn of(p: &Project, r: &Resolved) -> Structure {
        let mut ports: Vec<_> = r.ports.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        ports.sort_by(|a, b| a.0.cmp(&b.0));
        let mut clips: Vec<_> = r.clips.iter().map(|(id, path)| (id.clone(), path.clone())).collect();
        clips.sort();
        Structure {
            nodes: p.nodes.iter().map(|n| (n.id.clone(), n.kind)).collect(),
            edges: p.edges.iter().map(|e| (e.from.clone(), e.to.clone())).collect(),
            ports,
            clips,
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
    /// Подкачка длинных звуков кнопок (останавливается вместе с движком).
    _streamers: Vec<clips::Streamer>,
    /// Кнопки, чей файл не открылся при запуске.
    button_errors: HashMap<String, NodeStatus>,
}

enum Cmd {
    Project(Box<Project>),
    Enabled(bool),
    /// Настройка «Звук в VR (Virtual Desktop)»: устойчивый режим для его выхода.
    VdSteady(bool),
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
    last_gaps: HashMap<String, u32>,
    clip_cache: ClipCache,
    last_late: u32,
    /// Что SoundBimba поменяла в Windows («Оригинал звука») и куда сохранять журнал.
    journal: Journal,
    journal_path: PathBuf,
    /// Выход Virtual Desktop — в устойчивом режиме (настройка; остальные выходы всегда быстрые).
    vd_steady: bool,
    /// Настройку поменяли — перезапустить звук.
    restart_for_mode: bool,
}

impl Controller {
    fn stop(&mut self) {
        if self.running.take().is_some() {
            applog::info("audio stopped");
        }
        self.structure = None;
    }

    /// Желаемое перенаправление: exe → устройство (только пока звук включён).
    fn wanted_routes(&self) -> (BTreeMap<String, String>, Vec<(String, String)>) {
        let mut want = BTreeMap::new();
        let mut nodes = Vec::new();
        let Some(p) = self.project.as_ref().filter(|_| self.enabled) else { return (want, nodes) };
        for n in p.nodes.iter().filter(|n| n.kind == NodeKind::AppSource) {
            let mode = param(n, "original").unwrap_or("normal");
            if mode == "normal" {
                continue;
            }
            let (Some(exe), Some(device)) = (param(n, "exe"), param(n, "originalDevice")) else { continue };
            let exe = exe.to_lowercase();
            // Две программы-узла с одним exe: побеждает первый.
            want.entry(exe.clone()).or_insert_with(|| device.to_string());
            nodes.push((n.id.clone(), exe));
        }
        (want, nodes)
    }

    fn sync_routes(&mut self) {
        let (want, nodes) = self.wanted_routes();
        if want.is_empty() && self.journal.entries.is_empty() {
            self.snapshot.lock().unwrap().routes.clear();
            return;
        }
        let (states, changed) = routes::sync(&want, &mut self.journal, &WindowsRoutes);
        if changed {
            if let Err(e) = self.journal.save(&self.journal_path) {
                applog::error(&format!("routing: cannot save journal: {e}"));
            }
        }
        self.snapshot.lock().unwrap().routes =
            nodes.into_iter().filter_map(|(id, exe)| states.get(&exe).map(|s| (id, *s))).collect();
    }

    fn refresh(&mut self) {
        self.sync_routes();
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
        let resolved = resolve(&project, &mut self.clip_cache);
        let structure = Structure::of(&project, &resolved);
        let failed = self
            .running
            .as_ref()
            .is_some_and(|r| r.streams.iter().any(|(_, s, _)| s.as_ref().is_some_and(|s| s.failed().is_some())));
        let changed = self.structure.as_ref() != Some(&structure);
        let retry = failed && self.last_restart.elapsed() > RETRY_AFTER_FAILURE;
        let steady = std::mem::take(&mut self.restart_for_mode);
        if self.running.is_none() || changed || retry || steady {
            let why = if self.running.is_none() {
                "start"
            } else if changed {
                "graph or devices changed"
            } else if retry {
                "device failed, retrying"
            } else {
                "VR output mode changed"
            };
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
        self.last_gaps.clear();
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
            let mode = if self.vd_steady && resolved.vd_outputs.contains(id) {
                streams::RenderMode::Steady { buffer_ms: STEADY_BUFFER_MS }
            } else {
                streams::RenderMode::LowLatency
            };
            match streams::open_render_with(id, RENDER_RESERVE_MS, mode) {
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
        let mut buttons: HashMap<String, ButtonSource> = HashMap::new();
        let mut streamers = Vec::new();
        let mut button_errors = HashMap::new();
        for (id, path) in &resolved.clips {
            match clips::open(std::path::Path::new(path)) {
                Ok((source, streamer)) => {
                    buttons.insert(id.clone(), source);
                    streamers.extend(streamer);
                }
                Err(e) => {
                    applog::error(&format!("button {id}: {e}"));
                    button_errors.insert(id.clone(), clip_status(&e));
                }
            }
        }
        let graph = {
            let mut shared = self.shared.lock().unwrap();
            match Graph::compile(project, resolved, &plan, inputs, outputs, &mut buttons, &mut shared) {
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
        self.running = Some(Running { _runner: runner, late, streams: streams_info, _streamers: streamers, button_errors });
    }

    fn publish(&mut self, project: &Project, resolved: &Resolved) {
        let mut status = resolved.status.clone();
        let mut ports = Vec::new();

        if let Some(r) = &self.running {
            status.extend(r.button_errors.iter().map(|(id, s)| (id.clone(), *s)));
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
                let device_gaps = stream.as_ref().map(|s| s.stats.device_gaps()).unwrap_or_default();
                ports.push(PortReport {
                    port: port_name(key),
                    format: stream.as_ref().map(|s| s.info.format.clone()).unwrap_or_default(),
                    reserve_ms: stream.as_ref().map(|s| s.info.reserve_ms).unwrap_or_default(),
                    underruns,
                    overflows,
                    skips,
                    overloads,
                    device_gaps,
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
        for p in &ports {
            let before = self.last_gaps.insert(p.port.clone(), p.device_gaps).unwrap_or_default();
            if p.device_gaps > before {
                applog::info(&format!("audio device gaps on {}: +{}", p.port, p.device_gaps - before));
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
    shared: Arc<Mutex<HashMap<String, Arc<NodeShared>>>>,
}

impl Audio {
    pub fn start(app: AppHandle, journal_path: PathBuf) -> Audio {
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
            last_gaps: HashMap::new(),
            clip_cache: HashMap::new(),
            last_late: 0,
            journal: Journal::load(&journal_path),
            vd_steady: true,
            restart_for_mode: false,
            journal_path,
        };
        let control = std::thread::Builder::new()
            .name("sb-audio-control".into())
            .spawn(move || {
                if let Err(e) = winaudio::devices::ensure_com() {
                    applog::error(&format!("COM init failed in audio control: {e}"));
                }
                loop {
                    match rx.recv_timeout(TICK) {
                        // Движку и всему звуковому — схема с распакованными микросхемами.
                        Ok(Cmd::Project(p)) => c.project = Some(graph::flatten(&p)),
                        Ok(Cmd::Enabled(on)) => {
                            c.enabled = on;
                            applog::info(if on { "sound on" } else { "sound off" });
                        }
                        Ok(Cmd::VdSteady(on)) => {
                            if c.vd_steady != on {
                                c.vd_steady = on;
                                c.restart_for_mode = c.running.is_some();
                                applog::info(if on { "VR output: steady" } else { "VR output: fast" });
                            }
                        }
                        Ok(Cmd::Shutdown) => break,
                        Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                    c.refresh();
                }
                c.stop();
                // Закрываемся — всё, что меняли в Windows, возвращаем (что не удалось — вернём при
                // следующем запуске, журнал сохранён).
                c.enabled = false;
                c.sync_routes();
            })
            .expect("cannot start audio control thread");

        // Уровни и состояния — в интерфейс.
        let (snap, sh) = (snapshot.clone(), shared.clone());
        std::thread::Builder::new()
            .name("sb-audio-levels".into())
            .spawn(move || loop {
                std::thread::sleep(LEVELS_EVERY);
                let (levels, buttons, reductions, voices) = {
                    let shared = sh.lock().unwrap();
                    let levels: HashMap<String, f32> = shared.iter().map(|(id, s)| (id.clone(), s.take_peak())).collect();
                    let secs = |frames: u64| frames as f64 / RATE as f64;
                    let buttons: HashMap<String, ButtonEvent> = shared
                        .iter()
                        .filter_map(|(id, s)| {
                            s.button_state().map(|b| {
                                (id.clone(), ButtonEvent { playing: b.playing, pos: secs(b.pos_frames), len: secs(b.total_frames) })
                            })
                        })
                        .collect();
                    let reductions: HashMap<String, f32> = shared.iter().map(|(id, s)| (id.clone(), s.take_reduction())).filter(|(_, r)| *r > 0.0).collect();
                    let voices: HashMap<String, f32> = shared.iter().map(|(id, s)| (id.clone(), s.take_voice())).filter(|(_, v)| *v > 0.0).collect();
                    (levels, buttons, reductions, voices)
                };
                let snapshot = snap.lock().unwrap().clone();
                if app.emit("audio-state", StateEvent { snapshot: &snapshot, levels, buttons, reductions, voices }).is_err() {
                    break;
                }
            })
            .expect("cannot start audio levels thread");

        Audio { tx: Mutex::new(tx), snapshot, control: Mutex::new(Some(control)), shared }
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

    pub fn set_vd_steady(&self, on: bool) {
        let _ = self.tx.lock().unwrap().send(Cmd::VdSteady(on));
    }

    pub fn set_enabled(&self, on: bool) {
        let _ = self.tx.lock().unwrap().send(Cmd::Enabled(on));
    }

    /// Нажатие кнопки-звука (играть / стоп). Если звук выключен — ничего не происходит.
    pub fn press(&self, node_id: &str) {
        if let Some(sh) = self.shared.lock().unwrap().get(node_id) {
            sh.press();
        }
    }

    /// Перемотка кнопки на долю звука (0…1); если стояла — начинает играть оттуда.
    pub fn seek(&self, node_id: &str, fraction: f32) {
        if let Some(sh) = self.shared.lock().unwrap().get(node_id) {
            sh.seek(fraction);
        }
    }

    pub fn set_latency(&self, ms: f64) {
        self.snapshot.lock().unwrap().latency_ms = Some(ms);
    }

    pub fn snapshot(&self) -> Snapshot {
        self.snapshot.lock().unwrap().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::is_virtual_desktop;

    #[test]
    fn finds_virtual_desktop_output_by_name() {
        assert!(is_virtual_desktop("Speakers (Virtual Desktop Audio)"));
        assert!(is_virtual_desktop("Динамики (VIRTUAL DESKTOP Audio)"));
        assert!(!is_virtual_desktop("CABLE Input (VB-Audio Virtual Cable)"));
        assert!(!is_virtual_desktop("Speakers (Realtek(R) Audio)"));
    }
}
