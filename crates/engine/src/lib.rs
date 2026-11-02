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
pub use shared::{AtomicF32, NodeShared, PortStats};

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

enum Op {
    /// Источник: читает внешний вход. None — источник молчит.
    Source(Option<usize>),
    /// Громкость: сумма входов × усиление.
    Volume,
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
            NodeKind::SoundButton | NodeKind::Volume => continue,
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
    /// Собирает граф. `inputs`/`outputs` — открытые порты в порядке `PortPlan`; `shared` — общие
    /// с интерфейсом значения узлов (громкость, уровни), переживают перекомпиляцию.
    pub fn compile(
        project: &Project,
        resolver: &dyn PortResolver,
        plan: &PortPlan,
        inputs: Vec<RingReader>,
        outputs: Vec<RingWriter>,
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
                NodeKind::SoundButton => Op::Source(None), // кнопки — этап 3
                NodeKind::Volume => Op::Volume,
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
            match step.op {
                Op::Source(port) => match port {
                    Some(p) => out.copy_from_slice(&self.in_bufs[p][..]),
                    None => out.fill(0.0),
                },
                Op::Volume | Op::Output(_) => {
                    out.fill(0.0);
                    for &src in &step.inputs {
                        for (o, s) in out.iter_mut().zip(before[src].iter()) {
                            *o += *s;
                        }
                    }
                }
            }
            if let Op::Volume = step.op {
                let target = step.shared.target_gain();
                ramp_gain(&mut out[..], step.last_gain, target);
                step.last_gain = target;
            }
            if let Op::Output(Some(p)) = step.op {
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
