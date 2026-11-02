// SPDX-License-Identifier: AGPL-3.0-or-later
//! Значения, общие для потока звука и интерфейса: только атомарные операции, без блокировок.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering::Relaxed};

/// f32 в атомарной ячейке (через биты u32).
#[derive(Debug)]
pub struct AtomicF32(AtomicU32);

impl AtomicF32 {
    pub fn new(v: f32) -> Self {
        Self(AtomicU32::new(v.to_bits()))
    }
    pub fn load(&self) -> f32 {
        f32::from_bits(self.0.load(Relaxed))
    }
    pub fn store(&self, v: f32) {
        self.0.store(v.to_bits(), Relaxed)
    }
    /// Максимум для неотрицательных значений: у них порядок битов совпадает с порядком чисел.
    pub fn fetch_max_positive(&self, v: f32) {
        self.0.fetch_max(v.max(0.0).to_bits(), Relaxed);
    }
    pub fn swap(&self, v: f32) -> f32 {
        f32::from_bits(self.0.swap(v.to_bits(), Relaxed))
    }
}

/// Общие данные узла: громкость (пишет интерфейс), пиковый уровень (пишет поток звука),
/// для кнопки — запрос «нажали», играет ли и где сейчас (для кругового индикатора).
#[derive(Debug)]
pub struct NodeShared {
    gain: AtomicF32,
    mute: AtomicBool,
    peak: AtomicF32,
    press: AtomicBool,
    /// Запрос перемотки: доля звука 0…1; отрицательное — запроса нет.
    seek: AtomicF32,
    playing: AtomicBool,
    pos_frames: AtomicU64,
    total_frames: AtomicU64,
    /// Компрессор: настройки (пишет интерфейс) и наибольшее сжатие с прошлого опроса, дБ.
    comp: CompressorParams,
    reduction: AtomicF32,
}

/// Настройки компрессора (см. `graph::COMPRESSOR_RANGES`).
#[derive(Debug)]
pub struct CompressorParams {
    pub threshold_db: AtomicF32,
    pub ratio: AtomicF32,
    pub attack_ms: AtomicF32,
    pub release_ms: AtomicF32,
    pub makeup_db: AtomicF32,
}

/// Значения по умолчанию — те же, что у нового узла в интерфейсе.
pub const COMPRESSOR_DEFAULTS: [(&str, f32); 5] =
    [("threshold", -24.0), ("ratio", 3.0), ("attack", 5.0), ("release", 120.0), ("makeup", 12.0)];

impl Default for CompressorParams {
    fn default() -> Self {
        let d = |i: usize| AtomicF32::new(COMPRESSOR_DEFAULTS[i].1);
        CompressorParams { threshold_db: d(0), ratio: d(1), attack_ms: d(2), release_ms: d(3), makeup_db: d(4) }
    }
}

/// Состояние кнопки для интерфейса: позиция и длина в кадрах 48 кГц.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ButtonState {
    pub playing: bool,
    pub pos_frames: u64,
    pub total_frames: u64,
}

impl Default for NodeShared {
    fn default() -> Self {
        NodeShared {
            gain: AtomicF32::new(1.0),
            mute: AtomicBool::new(false),
            peak: AtomicF32::new(0.0),
            press: AtomicBool::new(false),
            seek: AtomicF32::new(-1.0),
            playing: AtomicBool::new(false),
            pos_frames: AtomicU64::new(0),
            total_frames: AtomicU64::new(0),
            comp: CompressorParams::default(),
            reduction: AtomicF32::new(0.0),
        }
    }
}

impl NodeShared {
    /// Берёт громкость из параметров узла (для узла «Громкость»).
    pub fn apply_params(&self, node: &graph::Node) {
        if node.kind == graph::NodeKind::Volume {
            let gain = node.params.get("gain").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
            self.gain.store(gain.clamp(0.0, 2.0));
            self.mute.store(node.params.get("mute").and_then(|v| v.as_bool()).unwrap_or(false), Relaxed);
        }
        if node.kind == graph::NodeKind::Compressor {
            let cells = [&self.comp.threshold_db, &self.comp.ratio, &self.comp.attack_ms, &self.comp.release_ms, &self.comp.makeup_db];
            for ((cell, (name, default)), (_, range)) in cells.into_iter().zip(COMPRESSOR_DEFAULTS).zip(graph::COMPRESSOR_RANGES) {
                let v = node.params.get(name).and_then(|v| v.as_f64()).unwrap_or(default as f64);
                cell.store(v.clamp(*range.start(), *range.end()) as f32);
            }
        }
    }

    pub fn compressor(&self) -> &CompressorParams {
        &self.comp
    }

    pub(crate) fn report_reduction(&self, db: f32) {
        self.reduction.fetch_max_positive(db);
    }

    /// Наибольшее сжатие компрессора (дБ) с прошлого вызова — для индикатора.
    pub fn take_reduction(&self) -> f32 {
        self.reduction.swap(0.0)
    }

    /// Усиление с учётом «Выключить звук».
    pub fn target_gain(&self) -> f32 {
        if self.mute.load(Relaxed) {
            0.0
        } else {
            self.gain.load()
        }
    }

    pub fn report_peak(&self, buf: &[f32]) {
        let peak = buf.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        self.peak.fetch_max_positive(peak);
    }

    /// Пик с прошлого вызова (интерфейс опрашивает несколько раз в секунду).
    pub fn take_peak(&self) -> f32 {
        self.peak.swap(0.0)
    }

    /// Кнопку нажали (играть, а если уже играет — стоп).
    pub fn press(&self) {
        self.press.store(true, Relaxed);
    }

    pub(crate) fn take_press(&self) -> bool {
        self.press.swap(false, Relaxed)
    }

    /// Перемотка: играть с доли `fraction` (0…1) звука, даже если кнопка стояла.
    pub fn seek(&self, fraction: f32) {
        self.seek.store(fraction.clamp(0.0, 1.0));
    }

    pub(crate) fn take_seek(&self) -> Option<f32> {
        let v = self.seek.swap(-1.0);
        (v >= 0.0).then_some(v)
    }

    pub(crate) fn set_button(&self, playing: bool, pos: u64, total: u64) {
        self.playing.store(playing, Relaxed);
        self.pos_frames.store(pos, Relaxed);
        self.total_frames.store(total, Relaxed);
    }

    /// Состояние кнопки; None — у узла нет звука (файл не выбран или не прочитался).
    pub fn button_state(&self) -> Option<ButtonState> {
        let total = self.total_frames.load(Relaxed);
        (total > 0).then(|| ButtonState {
            playing: self.playing.load(Relaxed),
            pos_frames: self.pos_frames.load(Relaxed),
            total_frames: total,
        })
    }

    /// Играет ли кнопка и какая доля звука уже прозвучала (0…1).
    pub fn playback(&self) -> Option<f32> {
        let s = self.button_state()?;
        s.playing.then(|| s.pos_frames as f32 / s.total_frames as f32)
    }
}

/// Счётчики порта: пропуски (нечего читать) и переполнения (некуда писать).
#[derive(Debug, Default)]
pub struct PortStats {
    pub underruns: AtomicU32,
    pub overflows: AtomicU32,
    pub skips: AtomicU32,
    /// Блоков, где сумма на выходе была громче максимума и сработал ограничитель.
    pub overloads: AtomicU32,
    /// Провалы на стороне Windows: захват сообщил о потере данных, или буфер вывода опустел
    /// (поток вывода не успел подложить звук).
    pub device_gaps: AtomicU32,
}

impl PortStats {
    pub fn snapshot(&self) -> (u32, u32, u32) {
        (self.underruns.load(Relaxed), self.overflows.load(Relaxed), self.skips.load(Relaxed))
    }

    pub fn overloads(&self) -> u32 {
        self.overloads.load(Relaxed)
    }

    pub fn device_gaps(&self) -> u32 {
        self.device_gaps.load(Relaxed)
    }
}
