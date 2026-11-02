// SPDX-License-Identifier: AGPL-3.0-or-later
//! Кольцевой буфер между двумя «часами» (устройство ↔ движок) с подстройкой скорости чтения.
//!
//! У каждого звукового устройства свой кварц: 48 000 Гц у одного — это 48 003 Гц у другого.
//! Без подстройки буфер со временем переполнится или опустеет — будут щелчки. Читающая сторона
//! держит заполнение около цели, чуть ускоряя или замедляя чтение (адаптивная передискретизация,
//! кубическая интерполяция). Она же переводит частоту устройства в частоту движка и обратно.

use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;

use rtrb::{Consumer, Producer, RingBuffer};

use crate::{PortStats, CHANNELS};

/// Наибольшая поправка скорости: ±0,5 % (расхождение кварцев обычно меньше 0,01 %).
const MAX_CORRECTION: f64 = 0.005;
/// Насколько сильно поправка реагирует на отклонение заполнения от цели.
const GAIN: f64 = 0.002;
/// Сглаживание замера заполнения (устройства отдают звук порциями).
const FILL_SMOOTHING: f64 = 0.02;

pub struct RingWriter {
    prod: Producer<f32>,
    stats: Arc<PortStats>,
}

pub struct RingReader {
    cons: Consumer<f32>,
    /// Входных кадров на один выходной без поправки (частота источника / частота читателя).
    nominal: f64,
    /// Дробная позиция между hist[1] и hist[2].
    pos: f64,
    /// Четыре последних кадра для интерполяции.
    hist: [[f32; CHANNELS]; 4],
    /// Целевое заполнение, кадров.
    target: usize,
    avg_fill: f64,
    primed: bool,
    stats: Arc<PortStats>,
}

/// Создаёт буфер. `src_rate` — частота пишущей стороны, `dst_rate` — читающей;
/// `target_frames` — сколько кадров держать про запас (это и есть добавленная задержка).
pub fn ring(capacity_frames: usize, src_rate: u32, dst_rate: u32, target_frames: usize) -> (RingWriter, RingReader) {
    let (prod, cons) = RingBuffer::new(capacity_frames.max(target_frames * 4) * CHANNELS);
    let stats = Arc::new(PortStats::default());
    (
        RingWriter { prod, stats: stats.clone() },
        RingReader {
            cons,
            nominal: src_rate as f64 / dst_rate as f64,
            pos: 0.0,
            hist: [[0.0; CHANNELS]; 4],
            target: target_frames.max(1),
            avg_fill: target_frames as f64,
            primed: false,
            stats,
        },
    )
}

impl RingWriter {
    /// Пишет кадры (стерео, по очереди). Если места нет — лишнее отбрасывается целыми кадрами.
    pub fn write(&mut self, samples: &[f32]) {
        let room = self.prod.slots() / CHANNELS * CHANNELS;
        let take = samples.len().min(room);
        for &s in &samples[..take] {
            let _ = self.prod.push(s);
        }
        if take < samples.len() {
            self.stats.overflows.fetch_add(1, Relaxed);
        }
    }

    pub fn stats(&self) -> &Arc<PortStats> {
        &self.stats
    }
}

fn hermite(y0: f32, y1: f32, y2: f32, y3: f32, t: f32) -> f32 {
    let c1 = 0.5 * (y2 - y0);
    let c2 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
    let c3 = 0.5 * (y3 - y0) + 1.5 * (y1 - y2);
    ((c3 * t + c2) * t + c1) * t + y1
}

impl RingReader {
    fn fill(&self) -> usize {
        self.cons.slots() / CHANNELS
    }

    fn pop_frame(&mut self) -> bool {
        if self.cons.slots() < CHANNELS {
            return false;
        }
        self.hist.copy_within(1.., 0);
        for ch in 0..CHANNELS {
            self.hist[3][ch] = self.cons.pop().unwrap_or(0.0);
        }
        true
    }

    /// Заполняет `out` (стерео, по очереди) кадрами с частотой читателя.
    pub fn read(&mut self, out: &mut [f32]) {
        let frames = out.len() / CHANNELS;
        let fill = self.fill();
        if !self.primed {
            // Ждём запаса, чтобы не захлебнуться на первых же порциях.
            if fill < self.target + 3 {
                out.fill(0.0);
                return;
            }
            // Всё, что накопилось сверх запаса, пока открывались устройства, — сразу отбрасываем:
            // иначе подстройка скорости съедала бы лишние десятки миллисекунд задержки секундами.
            for _ in 0..fill - self.target {
                self.pop_frame();
            }
            self.pos = 0.0;
            self.avg_fill = self.target as f64;
            self.primed = true;
        }
        // Скопилось слишком много (например, после зависания) — догоняем, отбросив лишнее.
        let fill = self.fill();
        if fill > self.target * 3 + frames * 2 {
            for _ in 0..fill - self.target {
                self.pop_frame();
            }
            self.avg_fill = self.target as f64;
            self.stats.skips.fetch_add(1, Relaxed);
        }
        self.avg_fill += (self.fill() as f64 - self.avg_fill) * FILL_SMOOTHING;
        let error = (self.avg_fill - self.target as f64) / self.target as f64;
        let ratio = self.nominal * (1.0 + (GAIN * error).clamp(-MAX_CORRECTION, MAX_CORRECTION));

        for f in 0..frames {
            let t = self.pos as f32;
            let h = &self.hist;
            for ch in 0..CHANNELS {
                out[f * CHANNELS + ch] = hermite(h[0][ch], h[1][ch], h[2][ch], h[3][ch], t);
            }
            self.pos += ratio;
            while self.pos >= 1.0 {
                if !self.pop_frame() {
                    // Пусто: остаток блока — тишина, дальше снова ждём запаса.
                    self.stats.underruns.fetch_add(1, Relaxed);
                    self.primed = false;
                    out[(f + 1) * CHANNELS..].fill(0.0);
                    return;
                }
                self.pos -= 1.0;
            }
        }
    }

    pub fn stats(&self) -> &PortStats {
        &self.stats
    }

    /// Текущее сглаженное заполнение, кадров (для диагностики и замера задержки).
    pub fn average_fill(&self) -> f64 {
        self.avg_fill
    }
}
