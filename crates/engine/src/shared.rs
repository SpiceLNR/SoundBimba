// SPDX-License-Identifier: AGPL-3.0-or-later
//! Значения, общие для потока звука и интерфейса: только атомарные операции, без блокировок.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};

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
    playing: AtomicBool,
    progress: AtomicF32,
}

impl Default for NodeShared {
    fn default() -> Self {
        NodeShared {
            gain: AtomicF32::new(1.0),
            mute: AtomicBool::new(false),
            peak: AtomicF32::new(0.0),
            press: AtomicBool::new(false),
            playing: AtomicBool::new(false),
            progress: AtomicF32::new(0.0),
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

    pub(crate) fn set_playback(&self, playing: bool, progress: f32) {
        self.playing.store(playing, Relaxed);
        self.progress.store(progress);
    }

    /// Играет ли кнопка и какая доля звука уже прозвучала (0…1).
    pub fn playback(&self) -> Option<f32> {
        self.playing.load(Relaxed).then(|| self.progress.load())
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
}

impl PortStats {
    pub fn snapshot(&self) -> (u32, u32, u32) {
        (self.underruns.load(Relaxed), self.overflows.load(Relaxed), self.skips.load(Relaxed))
    }

    pub fn overloads(&self) -> u32 {
        self.overloads.load(Relaxed)
    }
}
