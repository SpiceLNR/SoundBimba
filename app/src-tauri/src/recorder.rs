// SPDX-License-Identifier: AGPL-3.0-or-later
//! «Диктофон» (2026-11-19): запись того, что пришло на вход узла, и прослушивание мимо схемы.
//!
//! - Поток звука отдаёт отсчёты через буфер без ожидания (`engine::recorder`); здесь их забирает
//!   свой поток «sb-recorder» и пишет WAV (48 кГц, 16 бит, стерео) в
//!   `%APPDATA%\SoundBimba\recordings`: пока идёт запись — `<узел>.part.wav`, готовая — `<узел>.wav`.
//! - У каждого диктофона одна, последняя запись (до 60 мин); новая заменяет прежнюю, когда
//!   закончена. «Сохранить…» копирует её в выбранный файл (и помечает `<узел>.saved`).
//! - Перезапуск звука (провода переставили) запись не прерывает: новый граф ставит метку с тем же
//!   номером записи — пишем дальше в тот же файл.
//! - Программа закрылась посреди записи — при следующем запуске `.part.wav` дописывается в готовую.
//! - Прослушивание — отдельный маленький движок (кнопка → устройство), основную схему не трогает.

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use engine::recorder::{Piece, RecorderFeed};
use engine::{Graph, NodeShared, PortKey, PortResolver, CHANNELS, RATE};
use graph::{Edge, Node, NodeKind, Project};
use serde::Serialize;
use winaudio::{runner, streams};

use crate::applog;

/// Самая длинная запись: 60 мин (≈ 660 МБ), потом запись останавливается сама.
pub const MAX_SECONDS: u64 = 60 * 60;
const MAX_FRAMES: u64 = MAX_SECONDS * RATE as u64;
const TICK: Duration = Duration::from_millis(20);
const HEADER_BYTES: u64 = 44;
const BYTES_PER_FRAME: u64 = 2 * CHANNELS as u64;
/// Буфер устройства при прослушивании: задержка тут не важна, а провалов быть не должно.
const PLAY_BUFFER_MS: u32 = 40;
const PLAY_RESERVE_MS: f64 = 20.0;

/// Имя файла записи по id узла (у узлов внутри микросхем в id есть «/»).
pub fn file_stem(id: &str) -> String {
    id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect()
}

/// Состояние диктофона для интерфейса.
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RecorderInfo {
    pub recording: bool,
    /// Идёт запись — сколько записано; иначе — длина последней записи. Секунды.
    pub seconds: f64,
    /// Есть готовая запись (можно прослушать и сохранить).
    pub has_take: bool,
    pub saved: bool,
    /// В последней записи пропали куски (запись на диск не успевала).
    pub lost: bool,
    /// Ошибка записи (ключ локализации), если была.
    pub error: Option<String>,
}

/// Прослушивание: какая запись, где сейчас и сколько всего (секунды).
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PlayerInfo {
    pub id: String,
    pub pos: f64,
    pub len: f64,
}

struct Take {
    session: u32,
    file: BufWriter<File>,
    frames: u64,
    part: PathBuf,
    shared: Arc<NodeShared>,
    failed: bool,
}

enum Msg {
    Feed(String, Arc<NodeShared>, RecorderFeed),
    Nodes(Vec<String>),
    Shutdown,
}

struct Player {
    id: String,
    shared: Arc<NodeShared>,
    seen_playing: bool,
    // Порядок важен: движок останавливается раньше устройства и подкачки.
    _runner: runner::Runner,
    _stream: streams::Stream,
    _streamer: Option<clips::Streamer>,
}

pub struct Recorders {
    dir: PathBuf,
    tx: Mutex<Sender<Msg>>,
    info: Arc<Mutex<HashMap<String, RecorderInfo>>>,
    player: Arc<Mutex<Option<Player>>>,
    thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}

fn wav_header(frames: u64) -> [u8; HEADER_BYTES as usize] {
    let data = (frames * BYTES_PER_FRAME) as u32;
    let mut h = [0u8; HEADER_BYTES as usize];
    let mut put = |at: usize, bytes: &[u8]| h[at..at + bytes.len()].copy_from_slice(bytes);
    put(0, b"RIFF");
    put(4, &(36 + data).to_le_bytes());
    put(8, b"WAVEfmt ");
    put(16, &16u32.to_le_bytes());
    put(20, &1u16.to_le_bytes()); // PCM
    put(22, &(CHANNELS as u16).to_le_bytes());
    put(24, &RATE.to_le_bytes());
    put(28, &(RATE * BYTES_PER_FRAME as u32).to_le_bytes());
    put(32, &(BYTES_PER_FRAME as u16).to_le_bytes());
    put(34, &16u16.to_le_bytes());
    put(36, b"data");
    put(40, &data.to_le_bytes());
    h
}

/// Кадров в нашем WAV по размеру файла.
fn frames_in(path: &Path) -> Option<u64> {
    let len = fs::metadata(path).ok()?.len();
    (len >= HEADER_BYTES).then(|| (len - HEADER_BYTES) / BYTES_PER_FRAME)
}

/// Дописать заголовок по размеру файла и сделать запись готовой (`.part.wav` → `.wav`).
fn finish_file(part: &Path, frames: u64) -> io::Result<PathBuf> {
    {
        let mut f = fs::OpenOptions::new().write(true).open(part)?;
        f.seek(SeekFrom::Start(0))?;
        f.write_all(&wav_header(frames))?;
        f.flush()?;
    }
    let done = part.with_file_name(part.file_name().unwrap_or_default().to_string_lossy().replace(".part.wav", ".wav"));
    fs::rename(part, &done)?;
    let _ = fs::remove_file(done.with_extension("saved"));
    Ok(done)
}

impl Recorders {
    pub fn start(dir: PathBuf) -> Arc<Recorders> {
        if let Err(e) = fs::create_dir_all(&dir) {
            applog::error(&format!("recorder: cannot create {}: {e}", dir.display()));
        }
        recover(&dir);
        let (tx, rx) = channel();
        let r = Arc::new(Recorders {
            dir: dir.clone(),
            tx: Mutex::new(tx),
            info: Arc::default(),
            player: Arc::default(),
            thread: Mutex::new(None),
        });
        let writer = Writer { dir, feeds: Vec::new(), takes: HashMap::new(), info: r.info.clone(), player: r.player.clone() };
        let thread = std::thread::Builder::new()
            .name("sb-recorder".into())
            .spawn(move || writer.run(rx))
            .expect("cannot start the recorder thread");
        *r.thread.lock().unwrap() = Some(thread);
        r
    }

    /// Буфер записи нового графа для диктофона `id`.
    pub fn feed(&self, id: &str, shared: Arc<NodeShared>, feed: RecorderFeed) {
        let _ = self.tx.lock().unwrap().send(Msg::Feed(id.to_string(), shared, feed));
    }

    /// Какие диктофоны сейчас есть в схеме (удалённые останавливаются).
    pub fn set_nodes(&self, ids: Vec<String>) {
        let _ = self.tx.lock().unwrap().send(Msg::Nodes(ids));
    }

    pub fn info(&self) -> HashMap<String, RecorderInfo> {
        self.info.lock().unwrap().clone()
    }

    pub fn take_path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{}.wav", file_stem(id)))
    }

    /// Сохранить последнюю запись в выбранный файл.
    pub fn save(&self, id: &str, to: &Path) -> io::Result<()> {
        let from = self.take_path(id);
        fs::copy(&from, to)?;
        let _ = fs::write(from.with_extension("saved"), b"");
        if let Some(i) = self.info.lock().unwrap().get_mut(id) {
            i.saved = true;
        }
        applog::info(&format!("recorder {id}: saved to {}", to.display()));
        Ok(())
    }

    pub fn stop_playing(&self) {
        if self.player.lock().unwrap().take().is_some() {
            applog::info("recorder: listening stopped");
        }
    }

    /// Новая запись этого диктофона заменит файл — прослушивание его надо остановить.
    pub fn stop_playing_if(&self, id: &str) {
        let mut p = self.player.lock().unwrap();
        if p.as_ref().is_some_and(|p| p.id == id) {
            *p = None;
        }
    }

    pub fn player_info(&self) -> Option<PlayerInfo> {
        let p = self.player.lock().unwrap();
        let p = p.as_ref()?;
        let s = p.shared.button_state()?;
        let secs = |f: u64| f as f64 / RATE as f64;
        Some(PlayerInfo { id: p.id.clone(), pos: secs(s.pos_frames), len: secs(s.total_frames) })
    }

    pub fn seek(&self, fraction: f32) {
        if let Some(p) = self.player.lock().unwrap().as_ref() {
            p.shared.seek(fraction);
        }
    }

    /// Прослушать последнюю запись на устройстве `device` (id или «default») — мимо схемы.
    pub fn play(&self, id: &str, device: &str, devices: &[winaudio::devices::DeviceInfo]) -> Result<(), String> {
        self.stop_playing();
        let path = self.take_path(id);
        if !path.exists() {
            return Err("recorder-no-take".into());
        }
        let dev = match device {
            "" | "default" => winaudio::devices::default_device(devices, true),
            id => devices.iter().find(|d| d.render && d.id == id),
        }
        .ok_or("recorder-no-device")?;
        let (source, streamer) = clips::open(&path).map_err(|e| format!("cannot open the recording: {e}"))?;
        let key = PortKey::Render(dev.id.clone());
        let mut button = serde_json::Map::new();
        button.insert("file".into(), serde_json::Value::String(path.to_string_lossy().into_owned()));
        let project = Project {
            schema: graph::SCHEMA_BASE,
            nodes: vec![
                Node { id: "play".into(), kind: NodeKind::SoundButton, pos: [0.0, 0.0], params: button },
                Node { id: "out".into(), kind: NodeKind::SpeakerOut, pos: [0.0, 0.0], params: serde_json::Map::new() },
            ],
            edges: vec![Edge::audio("play", "out")],
        };
        let resolver = OnlyOut(key);
        let plan = engine::plan(&project, &resolver).map_err(|e| e.to_string())?;
        let (stream, writer) =
            streams::open_render_with(&dev.id, PLAY_RESERVE_MS, streams::RenderMode::Steady { buffer_ms: PLAY_BUFFER_MS })?;
        let mut buttons = HashMap::from([("play".to_string(), source)]);
        let mut shared = HashMap::new();
        let graph = Graph::compile(&project, &resolver, &plan, vec![], vec![writer], &mut buttons, &mut shared)
            .map_err(|e| e.to_string())?;
        let play = shared["play"].clone();
        play.press();
        let runner = runner::start(graph);
        applog::info(&format!("recorder {id}: listening on {}", dev.name));
        *self.player.lock().unwrap() =
            Some(Player { id: id.to_string(), shared: play, seen_playing: false, _runner: runner, _stream: stream, _streamer: streamer });
        Ok(())
    }

    pub fn shutdown(&self) {
        self.stop_playing();
        let _ = self.tx.lock().unwrap().send(Msg::Shutdown);
        if let Some(t) = self.thread.lock().unwrap().take() {
            let _ = t.join();
        }
    }
}

struct OnlyOut(PortKey);

impl PortResolver for OnlyOut {
    fn port_for(&self, node: &Node) -> Option<PortKey> {
        (node.id == "out").then(|| self.0.clone())
    }
}

/// Программа закрылась посреди записи: недописанные файлы делаем готовыми записями.
fn recover(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let path = e.path();
        if !path.to_string_lossy().ends_with(".part.wav") {
            continue;
        }
        let frames = frames_in(&path).unwrap_or(0);
        match finish_file(&path, frames) {
            Ok(done) => applog::info(&format!("recorder: recovered an unfinished recording {}", done.display())),
            Err(e) => applog::error(&format!("recorder: cannot recover {}: {e}", path.display())),
        }
    }
}

struct Writer {
    dir: PathBuf,
    feeds: Vec<(String, Arc<NodeShared>, RecorderFeed)>,
    takes: HashMap<String, Take>,
    info: Arc<Mutex<HashMap<String, RecorderInfo>>>,
    player: Arc<Mutex<Option<Player>>>,
}

impl Writer {
    fn run(mut self, rx: Receiver<Msg>) {
        loop {
            match rx.recv_timeout(TICK) {
                Ok(Msg::Feed(id, shared, feed)) => self.feeds.push((id, shared, feed)),
                Ok(Msg::Nodes(ids)) => self.set_nodes(ids),
                Ok(Msg::Shutdown) | Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => {}
            }
            self.tick();
        }
        // Закрываемся посреди записи — запись сохраняется готовой.
        for take in self.takes.values() {
            take.shared.set_recording(false);
        }
        self.tick();
    }

    fn set_nodes(&mut self, ids: Vec<String>) {
        let keep: HashSet<&String> = ids.iter().collect();
        for (id, take) in &self.takes {
            if !keep.contains(id) {
                applog::info(&format!("recorder {id}: node removed, recording stops"));
                take.shared.set_recording(false);
            }
        }
        let mut info = self.info.lock().unwrap();
        info.retain(|id, _| keep.contains(id));
        for id in ids {
            info.entry(id.clone()).or_insert_with(|| self.saved_info(&id));
        }
    }

    /// Что лежит на диске от прошлых записей этого диктофона.
    fn saved_info(&self, id: &str) -> RecorderInfo {
        let done = self.dir.join(format!("{}.wav", file_stem(id)));
        match frames_in(&done) {
            Some(frames) => RecorderInfo {
                seconds: frames as f64 / RATE as f64,
                has_take: true,
                saved: done.with_extension("saved").exists(),
                ..Default::default()
            },
            None => RecorderInfo::default(),
        }
    }

    fn tick(&mut self) {
        let Writer { dir, feeds, takes, info, player } = self;
        // Брошен ли буфер — до разбора: всё, что движок успел положить, тогда уже в нём.
        let gone: Vec<bool> = feeds.iter().map(|(_, _, f)| f.is_abandoned()).collect();
        for (id, shared, feed) in feeds.iter_mut() {
            feed.drain(|piece| match piece {
                Piece::Start(session) => {
                    if takes.get(id.as_str()).is_some_and(|t| t.session == session) {
                        return; // та же запись после перезапуска звука — пишем дальше
                    }
                    if let Some(old) = takes.remove(id.as_str()) {
                        finish(id, old, info, player);
                    }
                    match open_take(dir, id, session, shared.clone()) {
                        Ok(t) => {
                            applog::info(&format!("recorder {id}: recording"));
                            takes.insert(id.clone(), t);
                        }
                        Err(e) => {
                            applog::error(&format!("recorder {id}: cannot create the file: {e}"));
                            shared.set_recording(false);
                            set_error(info, id, "recorder-io");
                        }
                    }
                }
                Piece::Samples(x) => {
                    if let Some(t) = takes.get_mut(id.as_str()) {
                        write_samples(id, t, x, info);
                    }
                }
            });
        }
        // Буферы остановленных графов, из которых всё забрано, больше не нужны.
        let mut gone = gone.into_iter();
        feeds.retain(|_| !gone.next().unwrap_or(false));
        // Запись выключили (кнопкой, звук выключен, узел удалён, предел) — делаем готовой.
        let stopped: Vec<String> =
            takes.iter().filter(|(_, t)| t.failed || t.shared.recording() != (true, t.session)).map(|(id, _)| id.clone()).collect();
        for id in stopped {
            if let Some(t) = takes.remove(&id) {
                finish(&id, t, info, player);
            }
        }
        // Идущие записи — время для интерфейса.
        let mut i = info.lock().unwrap();
        for (id, t) in takes.iter() {
            let e = i.entry(id.clone()).or_default();
            e.recording = true;
            e.seconds = t.frames as f64 / RATE as f64;
            e.lost = t.shared.rec_dropped() > 0;
            e.error = None;
        }
        drop(i);
        // Дослушали до конца — устройство отпускаем.
        let mut p = player.lock().unwrap();
        if let Some(pl) = p.as_mut() {
            let playing = pl.shared.button_state().is_some_and(|s| s.playing);
            if playing {
                pl.seen_playing = true;
            } else if pl.seen_playing {
                *p = None;
            }
        }
    }
}

fn set_error(info: &Mutex<HashMap<String, RecorderInfo>>, id: &str, code: &str) {
    let mut i = info.lock().unwrap();
    let e = i.entry(id.to_string()).or_default();
    e.recording = false;
    e.error = Some(code.to_string());
}

fn open_take(dir: &Path, id: &str, session: u32, shared: Arc<NodeShared>) -> io::Result<Take> {
    let part = dir.join(format!("{}.part.wav", file_stem(id)));
    let mut file = BufWriter::new(File::create(&part)?);
    file.write_all(&wav_header(0))?;
    Ok(Take { session, file, frames: 0, part, shared, failed: false })
}

fn write_samples(id: &str, t: &mut Take, x: &[f32], info: &Mutex<HashMap<String, RecorderInfo>>) {
    if t.failed || t.frames >= MAX_FRAMES {
        return;
    }
    let room = ((MAX_FRAMES - t.frames) as usize) * CHANNELS;
    let x = &x[..x.len().min(room)];
    let mut bytes = Vec::with_capacity(x.len() * 2);
    for &v in x {
        bytes.extend_from_slice(&((v.clamp(-1.0, 1.0) * 32767.0).round() as i16).to_le_bytes());
    }
    if let Err(e) = t.file.write_all(&bytes) {
        applog::error(&format!("recorder {id}: cannot write: {e}"));
        t.failed = true;
        t.shared.set_recording(false);
        set_error(info, id, "recorder-io");
        return;
    }
    t.frames += (x.len() / CHANNELS) as u64;
    if t.frames >= MAX_FRAMES {
        applog::info(&format!("recorder {id}: reached {} minutes, recording stops", MAX_SECONDS / 60));
        t.shared.set_recording(false);
    }
}

fn finish(id: &str, mut t: Take, info: &Mutex<HashMap<String, RecorderInfo>>, player: &Mutex<Option<Player>>) {
    // Файл готовой записи заменяется — прослушивание прежней останавливаем.
    {
        let mut p = player.lock().unwrap();
        if p.as_ref().is_some_and(|p| p.id == id) {
            *p = None;
        }
    }
    let flushed = t.file.flush();
    drop(t.file);
    let result = flushed.and_then(|_| finish_file(&t.part, t.frames));
    let lost = t.shared.rec_dropped() > 0;
    let mut i = info.lock().unwrap();
    let e = i.entry(id.to_string()).or_default();
    e.recording = false;
    match result {
        Ok(_) => {
            applog::info(&format!(
                "recorder {id}: recorded {:.1} s{}",
                t.frames as f64 / RATE as f64,
                if lost { ", some audio was lost (disk too slow)" } else { "" }
            ));
            *e = RecorderInfo { seconds: t.frames as f64 / RATE as f64, has_take: true, lost, ..Default::default() };
        }
        Err(err) => {
            applog::error(&format!("recorder {id}: cannot finish the file: {err}"));
            e.error = Some("recorder-io".into());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("sb-recorder-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn writer(dir: &Path) -> Writer {
        Writer { dir: dir.to_path_buf(), feeds: Vec::new(), takes: HashMap::new(), info: Arc::default(), player: Arc::default() }
    }

    /// Пишем в буфер тем же кодом, что движок (метка, затем отсчёты). `marked` — память движка.
    fn push(tap: &mut engine::recorder::RecorderTap, marked: &mut u32, shared: &NodeShared, blocks: usize, v: f32) {
        for _ in 0..blocks {
            engine::recorder::record(Some(tap), marked, shared, &[v; engine::BLOCK_SAMPLES]);
        }
    }

    #[test]
    fn file_names_are_safe() {
        assert_eq!(file_stem("n12"), "n12");
        assert_eq!(file_stem("n35/n2"), "n35_n2");
        assert_eq!(file_stem("..\\x:y"), "___x_y");
    }

    #[test]
    fn a_take_becomes_a_wav_and_a_new_take_replaces_it() {
        let dir = temp_dir("take");
        let mut w = writer(&dir);
        let shared = Arc::new(NodeShared::default());
        let (mut tap, feed) = engine::recorder::tap();
        let mut marked = 0;
        w.feeds.push(("n1".into(), shared.clone(), feed));
        w.set_nodes(vec!["n1".into()]);
        shared.set_recording(true);
        push(&mut tap, &mut marked, &shared, 200, 0.5); // 1 с
        w.tick();
        assert!(w.info.lock().unwrap()["n1"].recording);
        shared.set_recording(false);
        w.tick();
        let info = w.info.lock().unwrap()["n1"].clone();
        assert!(!info.recording && info.has_take && !info.saved && (info.seconds - 1.0).abs() < 1e-6, "{info:?}");
        let wav = dir.join("n1.wav");
        let bytes = fs::read(&wav).unwrap();
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(bytes.len() as u64, HEADER_BYTES + RATE as u64 * BYTES_PER_FRAME);
        assert_eq!(u32::from_le_bytes(bytes[40..44].try_into().unwrap()) as u64, RATE as u64 * BYTES_PER_FRAME);
        assert_eq!(i16::from_le_bytes([bytes[44], bytes[45]]), 16384); // 0,5
        assert!(!dir.join("n1.part.wav").exists());
        // Сохранили — пометка; новая запись — пометка снимается, файл заменяется.
        let out = dir.join("saved-copy.wav");
        let r = Recorders { dir: dir.clone(), tx: Mutex::new(channel().0), info: w.info.clone(), player: Arc::default(), thread: Mutex::new(None) };
        r.save("n1", &out).unwrap();
        assert!(w.info.lock().unwrap()["n1"].saved && out.exists());
        shared.set_recording(true);
        push(&mut tap, &mut marked, &shared, 100, 0.25);
        w.tick();
        shared.set_recording(false);
        w.tick();
        let info = w.info.lock().unwrap()["n1"].clone();
        assert!(!info.saved && (info.seconds - 0.5).abs() < 1e-6, "{info:?}");
        assert_eq!(fs::read(&out).unwrap().len(), bytes.len(), "сохранённая копия не тронута");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn recording_continues_into_the_same_file_after_a_restart() {
        let dir = temp_dir("restart");
        let mut w = writer(&dir);
        let shared = Arc::new(NodeShared::default());
        shared.set_recording(true);
        let (mut tap, feed) = engine::recorder::tap();
        let mut marked = 0;
        w.feeds.push(("n1".into(), shared.clone(), feed));
        push(&mut tap, &mut marked, &shared, 100, 0.1);
        drop(tap); // старый граф удалён
        w.tick();
        let (mut tap, feed) = engine::recorder::tap();
        let mut marked = 0;
        w.feeds.push(("n1".into(), shared.clone(), feed));
        push(&mut tap, &mut marked, &shared, 100, 0.1);
        w.tick();
        assert_eq!(w.feeds.len(), 1, "брошенный буфер убран");
        shared.set_recording(false);
        w.tick();
        assert!((w.info.lock().unwrap()["n1"].seconds - 1.0).abs() < 1e-6, "обе половины в одной записи");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn removed_node_and_closing_finish_the_recording() {
        let dir = temp_dir("removed");
        let mut w = writer(&dir);
        let shared = Arc::new(NodeShared::default());
        shared.set_recording(true);
        let (mut tap, feed) = engine::recorder::tap();
        let mut marked = 0;
        w.feeds.push(("n1".into(), shared.clone(), feed));
        push(&mut tap, &mut marked, &shared, 10, 0.1);
        w.tick();
        w.set_nodes(vec![]);
        w.tick();
        assert!(!shared.recording().0 && dir.join("n1.wav").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn unfinished_recording_is_recovered_on_start() {
        let dir = temp_dir("recover");
        let part = dir.join("n7.part.wav");
        let mut bytes = wav_header(0).to_vec();
        bytes.extend(std::iter::repeat_n(0u8, (RATE as u64 * BYTES_PER_FRAME / 2) as usize)); // 0,5 с
        fs::write(&part, bytes).unwrap();
        recover(&dir);
        assert!(!part.exists());
        let w = writer(&dir);
        let info = w.saved_info("n7");
        assert!(info.has_take && (info.seconds - 0.5).abs() < 1e-6, "{info:?}");
        let _ = fs::remove_dir_all(&dir);
    }

    /// На железе: прослушивание (отдельный маленький движок) открывает устройство по умолчанию и
    /// идёт по записи. Запись — тишина, в колонках ничего не слышно.
    /// `cargo test -p soundbimba listening_plays_on_a_real_device -- --ignored --nocapture`
    #[test]
    #[ignore = "нужно звуковое устройство"]
    fn listening_plays_on_a_real_device() {
        let dir = temp_dir("listen");
        let mut bytes = wav_header(RATE as u64 * 2).to_vec();
        bytes.extend(std::iter::repeat_n(0u8, (RATE as u64 * 2 * BYTES_PER_FRAME) as usize)); // 2 с тишины
        fs::write(dir.join("n1.wav"), bytes).unwrap();
        winaudio::devices::ensure_com().unwrap();
        let devs = winaudio::devices::list().unwrap();
        let r = Recorders::start(dir.clone());
        r.play("n1", "default", &devs).expect("прослушивание запустилось");
        std::thread::sleep(Duration::from_millis(800));
        let p = r.player_info().expect("идёт прослушивание");
        println!("прослушивание: {:.2} с из {:.2} с", p.pos, p.len);
        assert!((p.len - 2.0).abs() < 0.01 && p.pos > 0.3, "{p:?}");
        r.seek(0.9);
        std::thread::sleep(Duration::from_millis(800));
        assert!(r.player_info().is_none(), "дослушали — устройство отпущено");
        r.shutdown();
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn recording_stops_at_the_limit() {
        let dir = temp_dir("limit");
        let shared = Arc::new(NodeShared::default());
        shared.set_recording(true);
        let info: Mutex<HashMap<String, RecorderInfo>> = Mutex::default();
        let mut t = open_take(&dir, "n1", shared.recording().1, shared.clone()).unwrap();
        t.frames = MAX_FRAMES - 10;
        write_samples("n1", &mut t, &[0.0; 100], &info);
        assert_eq!(t.frames, MAX_FRAMES);
        assert!(!shared.recording().0);
        drop(t);
        let _ = fs::remove_dir_all(&dir);
    }
}
