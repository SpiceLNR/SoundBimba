// SPDX-License-Identifier: AGPL-3.0-or-later
//! Аудио-движок SoundBimba (раздел 5 архитектуры): одна частота на весь граф (48 кГц, float32,
//! стерео), граф «компилируется» в порядок обработки и набор буферов, входы складываются,
//! громкость — отдельный узел. Между движком и устройствами — кольцевые буферы с подстройкой
//! скорости (у каждого устройства свои часы).
//!
//! Правило реального времени: в `Graph::process` нет выделения памяти, блокировок и ввода-вывода.
//! Всё выделяется при компиляции; с интерфейсом движок общается через атомарные значения.

pub mod ring;
pub mod shared;

use std::collections::HashMap;
use std::sync::Arc;

use graph::{NodeKind, Project};

pub use ring::{ring, RingReader, RingWriter};
pub mod compressor;
pub mod morpher;

pub use shared::{AtomicF32, ButtonState, CompressorParams, NodeShared, PortStats, COMPRESSOR_DEFAULTS};

/// Частота дискретизации движка.
pub const RATE: u32 = 48_000;
/// Число каналов внутри движка.
pub const CHANNELS: usize = 2;
/// Размер блока обработки: 240 кадров = 5 мс.
pub const BLOCK: usize = 240;
/// Отсчётов в блоке (кадры × каналы).
pub const BLOCK_SAMPLES: usize = BLOCK * CHANNELS;

/// Что должно быть подключено снаружи, чтобы граф звучал.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PortKey {
    /// Захват звука процесса (вместе с дочерними) по PID.
    Process(u32),
    /// Захват с устройства ввода (id устройства Windows).
    Capture(String),
    /// Вывод на устройство (id устройства Windows).
    Render(String),
}

/// Как узлы проекта превращаются во внешние порты. Решает приложение (какой PID у exe, какое
/// устройство «по умолчанию»), движок только применяет.
pub trait PortResolver {
    /// Порт для узла-источника или выхода; None — узел молчит (программа не запущена, нет устройства).
    fn port_for(&self, node: &graph::Node) -> Option<PortKey>;

}

/// Звук кнопки. Начало лежит в памяти (звук стартует мгновенно), остальное — если файл длинный —
/// подкачивается с диска через кольцевой буфер `tail`.
pub struct ButtonSource {
    /// Первые секунды звука: 48 кГц, стерео, отсчёты по очереди.
    pub head: Arc<[f32]>,
    /// Длина всего звука, кадров 48 кГц.
    pub total_frames: u64,
    pub tail: Option<TailFeed>,
}

/// Подкачка остального звука. В буфере — отсчёты стерео; метка перемотки — пара (NaN, номер
/// просьбы): всё после неё идёт с позиции, которую движок попросил под этим номером.
pub struct TailFeed {
    pub cons: rtrb::Consumer<f32>,
    pub ctl: Arc<TailControl>,
}

/// Просьба движка к подкачке: «с какого кадра» и номер просьбы; ответ подкачки — «файл кончился»
/// (номер просьбы, для которой отдано всё до конца файла).
#[derive(Debug, Default)]
pub struct TailControl {
    pub want_gen: std::sync::atomic::AtomicU32,
    pub want_pos: std::sync::atomic::AtomicU64,
    pub eof_gen: std::sync::atomic::AtomicU32,
}

/// Затухание при паузе и плавное вступление звука — без щелчков: 5 мс.
const STOP_FADE: usize = RATE as usize / 200;

/// Запас подкачки перед игрой после перемотки (или если диск не успел): 0,1 с. Иначе звук
/// начинает играть, едва придут первые отсчёты, и тут же «голодает» — слышно прерывание.
const TAIL_PREFILL: u64 = RATE as u64 / 10;

/// Состояние проигрывания кнопки (живёт в потоке звука).
#[derive(Default)]
struct Playback {
    pos: u64,
    playing: bool,
    /// На паузе: не играет, но позиция сохранена.
    paused: bool,
    /// Сколько кадров затухания осталось; Some — сейчас затухаем перед паузой.
    fade_left: Option<usize>,
    /// Сколько кадров плавного вступления осталось.
    fade_in: usize,
    /// Номер последней просьбы к подкачке и пришла ли уже её метка.
    gen: u32,
    synced: bool,
    /// Ждём запаса подкачки (после перемотки или нехватки данных).
    buffering: bool,
}

enum Op {
    /// Источник: читает внешний вход. None — источник молчит.
    Source(Option<usize>),
    /// Кнопка-звук: проигрывает звук по нажатию, с перемоткой.
    Button(Option<ButtonSource>, Playback),
    /// Громкость: сумма входов × усиление.
    Volume,
    /// Компрессор: сумма входов, громкое сжимается, всё вместе поднимается.
    Compressor(compressor::State),
    /// Морфер голоса: сумма входов → высота, робот, рация, эхо.
    Morpher(Box<morpher::State>),
    /// Выход: сумма входов → внешний выход. None — выхода нет.
    Output(Option<usize>),
}

struct Step {
    op: Op,
    /// Индексы (в `steps`) узлов, чьи выходы складываются на входе этого узла.
    inputs: Vec<usize>,
    shared: Arc<NodeShared>,
    /// Усиление, применённое в прошлом блоке (для плавного изменения без щелчков).
    last_gain: f32,
}

/// Скомпилированный граф. Создаётся вне потока реального времени.
pub struct Graph {
    steps: Vec<Step>,
    buffers: Vec<[f32; BLOCK_SAMPLES]>,
    inputs: Vec<RingReader>,
    outputs: Vec<RingWriter>,
    /// Блок каждого входа: читается один раз, даже если устройство у нескольких узлов.
    in_bufs: Vec<[f32; BLOCK_SAMPLES]>,
    /// Сумма для каждого выхода: несколько узлов на одном устройстве складываются.
    out_bufs: Vec<[f32; BLOCK_SAMPLES]>,
}

/// Описание портов, которые нужно открыть для скомпилированного графа (по порядку индексов).
pub struct PortPlan {
    pub inputs: Vec<PortKey>,
    pub outputs: Vec<PortKey>,
}

/// Порядок обработки и порты графа. Ошибка — граф некорректен (цикл и т.п.).
pub fn plan(project: &Project, resolver: &dyn PortResolver) -> Result<PortPlan, graph::GraphError> {
    graph::validate(project)?;
    let mut inputs: Vec<PortKey> = Vec::new();
    let mut outputs: Vec<PortKey> = Vec::new();
    for n in &project.nodes {
        let list = match n.kind {
            NodeKind::AppSource | NodeKind::MicInput => &mut inputs,
            NodeKind::SpeakerOut | NodeKind::VirtualOut => &mut outputs,
            NodeKind::SoundButton | NodeKind::Volume | NodeKind::Compressor | NodeKind::VoiceMorpher => continue,
        };
        // Два узла на одном устройстве делят один порт.
        if let Some(key) = resolver.port_for(n) {
            if !list.contains(&key) {
                list.push(key);
            }
        }
    }
    Ok(PortPlan { inputs, outputs })
}

impl Graph {
    /// Собирает граф. `inputs`/`outputs` — открытые порты в порядке `PortPlan`; `buttons` — звуки
    /// кнопок по id узла (забираются в граф); `shared` — общие с интерфейсом значения узлов
    /// (громкость, уровни, кнопки), переживают перекомпиляцию.
    pub fn compile(
        project: &Project,
        resolver: &dyn PortResolver,
        plan: &PortPlan,
        inputs: Vec<RingReader>,
        outputs: Vec<RingWriter>,
        buttons: &mut HashMap<String, ButtonSource>,
        shared: &mut HashMap<String, Arc<NodeShared>>,
    ) -> Result<Graph, graph::GraphError> {
        let order = graph::topo_order(project)?;
        let index: HashMap<&str, usize> = order.iter().enumerate().map(|(i, id)| (*id, i)).collect();
        let by_id: HashMap<&str, &graph::Node> = project.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
        let mut steps = Vec::with_capacity(order.len());
        for id in &order {
            let node = by_id[id];
            let sh = shared.entry(node.id.clone()).or_insert_with(|| Arc::new(NodeShared::default())).clone();
            sh.apply_params(node);
            let port = resolver.port_for(node);
            let op = match node.kind {
                NodeKind::AppSource | NodeKind::MicInput => {
                    Op::Source(port.and_then(|k| plan.inputs.iter().position(|p| *p == k)))
                }
                NodeKind::SoundButton => Op::Button(buttons.remove(&node.id), Playback::default()),
                NodeKind::Volume => Op::Volume,
                NodeKind::Compressor => Op::Compressor(compressor::State::new(sh.compressor())),
                NodeKind::VoiceMorpher => Op::Morpher(Box::default()),
                NodeKind::SpeakerOut | NodeKind::VirtualOut => {
                    Op::Output(port.and_then(|k| plan.outputs.iter().position(|p| *p == k)))
                }
            };
            let mut ins: Vec<usize> =
                project.edges.iter().filter(|e| e.to == node.id).map(|e| index[e.from.as_str()]).collect();
            ins.sort_unstable();
            steps.push(Step { op, inputs: ins, last_gain: sh.target_gain(), shared: sh });
        }
        // Узлы, которых больше нет в проекте, забываем.
        shared.retain(|id, _| by_id.contains_key(id.as_str()));
        let block = || [0.0; BLOCK_SAMPLES];
        let buffers = (0..steps.len()).map(|_| block()).collect();
        let in_bufs = (0..inputs.len()).map(|_| block()).collect();
        let out_bufs = (0..outputs.len()).map(|_| block()).collect();
        Ok(Graph { steps, buffers, inputs, outputs, in_bufs, out_bufs })
    }

    /// Обрабатывает один блок (BLOCK кадров). Реальное время: без выделения памяти и блокировок.
    pub fn process(&mut self) {
        for (reader, buf) in self.inputs.iter_mut().zip(self.in_bufs.iter_mut()) {
            reader.read(&mut buf[..]);
        }
        for i in 0..self.steps.len() {
            // Буфер этого узла отделяем от остальных, чтобы читать входы без копий.
            let (before, rest) = self.buffers.split_at_mut(i);
            let out = &mut rest[0];
            let step = &mut self.steps[i];
            match &mut step.op {
                Op::Source(port) => match *port {
                    Some(p) => out.copy_from_slice(&self.in_bufs[p][..]),
                    None => out.fill(0.0),
                },
                Op::Button(src, pb) => play_button(src.as_mut(), pb, &step.shared, &mut out[..]),
                Op::Volume | Op::Compressor(_) | Op::Morpher(_) | Op::Output(_) => {
                    out.fill(0.0);
                    for &src in &step.inputs {
                        for (o, s) in out.iter_mut().zip(before[src].iter()) {
                            *o += *s;
                        }
                    }
                }
            }
            if let Op::Compressor(state) = &mut step.op {
                let reduction = state.process(&mut out[..], step.shared.compressor());
                step.shared.report_reduction(reduction);
            }
            if let Op::Morpher(state) = &mut step.op {
                state.process(&mut out[..], step.shared.morpher());
            }
            if let Op::Volume = step.op {
                let target = step.shared.target_gain();
                ramp_gain(&mut out[..], step.last_gain, target);
                step.last_gain = target;
            }
            if let Op::Output(Some(p)) = &step.op {
                let p = *p;
                for (m, s) in self.out_bufs[p].iter_mut().zip(out.iter()) {
                    *m += *s;
                }
            }
            step.shared.report_peak(&out[..]);
        }
        for (writer, buf) in self.outputs.iter_mut().zip(self.out_bufs.iter_mut()) {
            if soft_limit(&mut buf[..]) {
                writer.stats().overloads.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            writer.write(&buf[..]);
            buf.fill(0.0);
        }
    }

    /// Статистика портов (пропуски, переполнения) — для диагностики.
    pub fn input_stats(&self) -> impl Iterator<Item = &PortStats> {
        self.inputs.iter().map(|r| r.stats())
    }
}

/// Начать играть с кадра `pos`; если позиция за пределами начала в памяти — попросить подкачку.
fn start_button(src: &ButtonSource, pb: &mut Playback, pos: u64) {
    use std::sync::atomic::Ordering::Release;
    pb.pos = pos.min(src.total_frames);
    pb.playing = true;
    pb.paused = false;
    pb.fade_left = None;
    pb.fade_in = STOP_FADE;
    if let Some(tail) = &src.tail {
        let head_frames = (src.head.len() / CHANNELS) as u64;
        pb.gen = pb.gen.wrapping_add(1).max(1);
        pb.synced = false;
        pb.buffering = true;
        tail.ctl.want_pos.store(pb.pos.max(head_frames), Release);
        tail.ctl.want_gen.store(pb.gen, Release);
    }
}

/// Один блок кнопки: нажатие — играть, повторное — пауза (с затуханием), ещё раз — дальше с того
/// же места; перемотка — играть с указанной доли звука; в конце — стоп и шкала на начало.
fn play_button(src: Option<&mut ButtonSource>, pb: &mut Playback, shared: &NodeShared, out: &mut [f32]) {
    out.fill(0.0);
    let pressed = shared.take_press();
    let seek = shared.take_seek();
    let Some(src) = src else {
        shared.set_button(false, 0, 0);
        return;
    };
    let total = src.total_frames;
    if let Some(frac) = seek {
        start_button(src, pb, (frac.clamp(0.0, 1.0) as f64 * total as f64) as u64);
    } else if pressed {
        if pb.playing && pb.fade_left.is_none() {
            pb.fade_left = Some(STOP_FADE);
        } else if pb.paused {
            start_button(src, pb, pb.pos);
        } else if !pb.playing {
            start_button(src, pb, 0);
        }
    }
    let head_frames = (src.head.len() / CHANNELS) as u64;
    // Ждём метку свежей просьбы: всё, что подкачка успела положить раньше, — выбрасываем.
    if let Some(tail) = &mut src.tail {
        while !pb.synced {
            let Ok(v) = tail.cons.pop() else { break };
            if v.is_nan() {
                if let Ok(g) = tail.cons.pop() {
                    pb.synced = g as u32 == pb.gen;
                }
            }
        }
    }
    for f in 0..out.len() / CHANNELS {
        if !pb.playing {
            break;
        }
        let mut gain = match pb.fade_left {
            Some(0) => {
                // Затухли — пауза: позиция остаётся.
                pb.playing = false;
                pb.paused = true;
                break;
            }
            Some(n) => {
                pb.fade_left = Some(n - 1);
                n as f32 / STOP_FADE as f32
            }
            None => 1.0,
        };
        if pb.pos >= total {
            pb.playing = false;
            break;
        }
        let (l, r) = if pb.pos < head_frames {
            let i = pb.pos as usize * CHANNELS;
            (src.head[i], src.head[i + 1])
        } else {
            match &mut src.tail {
                Some(tail) => {
                    // Подкачка ещё не ответила на перемотку — тишина, позиция стоит.
                    if !pb.synced {
                        break;
                    }
                    let avail = (tail.cons.slots() / CHANNELS) as u64;
                    // Подкачка отдала всё до конца файла (длина в заголовке бывает чуть больше).
                    let file_ended = tail.ctl.eof_gen.load(std::sync::atomic::Ordering::Acquire) == pb.gen;
                    if pb.buffering {
                        // Ждём запаса (или всего, что осталось до конца), потом — плавно.
                        if avail < TAIL_PREFILL.min(total - pb.pos) && !file_ended {
                            break;
                        }
                        pb.buffering = false;
                        pb.fade_in = STOP_FADE;
                    }
                    if avail == 0 {
                        if file_ended {
                            pb.pos = total; // звук кончился — дальше обычный конец трека
                            break;
                        }
                        pb.buffering = true; // диск не успел — подождём запаса, а не будем заикаться
                        break;
                    }
                    (tail.cons.pop().unwrap_or(0.0), tail.cons.pop().unwrap_or(0.0))
                }
                None => {
                    pb.playing = false;
                    break;
                }
            }
        };
        if pb.fade_in > 0 {
            gain *= 1.0 - pb.fade_in as f32 / STOP_FADE as f32;
            pb.fade_in -= 1;
        }
        out[f * CHANNELS] = l * gain;
        out[f * CHANNELS + 1] = r * gain;
        pb.pos += 1;
    }
    if pb.pos >= total {
        // Доиграла до конца: стоп, шкала — на начало.
        pb.playing = false;
        pb.paused = false;
        pb.pos = 0;
    }
    if !pb.playing {
        pb.fade_left = None;
    }
    let shown = if pb.playing || pb.paused { pb.pos } else { 0 };
    shared.set_button(pb.playing, shown, total);
}

/// Порог мягкого ограничителя: тише — звук не меняется вовсе.
const LIMIT_KNEE: f32 = 0.9;

/// Мягкий ограничитель на выходе: громкие пики (сумма источников, усиление выше 100 %) плавно
/// прижимаются к максимуму вместо жёсткого среза — жёсткий срез слышен как хруст.
/// Возвращает true, если в блоке был перегруз (звук громче максимума).
fn soft_limit(buf: &mut [f32]) -> bool {
    let mut overloaded = false;
    for s in buf.iter_mut() {
        let a = s.abs();
        if a > LIMIT_KNEE {
            overloaded |= a > 1.0;
            let room = 1.0 - LIMIT_KNEE;
            *s = s.signum() * (LIMIT_KNEE + room * ((a - LIMIT_KNEE) / room).tanh());
        }
    }
    overloaded
}

/// Усиление с плавным переходом от `from` к `to` за блок — без щелчков при движении ползунка.
fn ramp_gain(buf: &mut [f32], from: f32, to: f32) {
    let frames = buf.len() / CHANNELS;
    if (from - to).abs() < 1e-6 {
        if (to - 1.0).abs() > 1e-6 {
            buf.iter_mut().for_each(|s| *s *= to);
        }
        return;
    }
    for (f, frame) in buf.as_chunks_mut::<CHANNELS>().0.iter_mut().enumerate() {
        let g = from + (to - from) * (f + 1) as f32 / frames as f32;
        frame.iter_mut().for_each(|s| *s *= g);
    }
}

#[cfg(test)]
mod tests;
