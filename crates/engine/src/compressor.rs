// SPDX-License-Identifier: AGPL-3.0-or-later
//! Компрессор: громкое делает тише, после чего весь звук поднимается («усиление»). Так тихое
//! становится громче без перегруза — голос звучит ровнее.
//!
//! Устройство обычное (feed-forward): уровень считается по громкому из двух каналов (каналы
//! сжимаются одинаково, стерео не «плывёт»), мягкое колено 6 дБ, сглаживание сжатия в децибелах
//! с разной скоростью: атака — как быстро сжимать, восстановление — как быстро отпускать.
//! Реальное время: без выделения памяти, только арифметика.

use crate::{CompressorParams, CHANNELS, RATE};

/// Ширина мягкого колена, дБ: сжатие нарастает плавно около порога, без «ступеньки».
const KNEE_DB: f32 = 6.0;
/// Тише этого уровень не считаем (логарифм нуля).
const FLOOR: f32 = 1e-6;

/// Состояние компрессора между блоками.
pub struct State {
    /// Текущее сжатие, дБ (0 — не сжимает).
    reduction_db: f32,
    /// Усиление прошлого блока, дБ: при движении ползунка меняется плавно, без щелчка.
    last_makeup_db: f32,
}

impl State {
    pub fn new(params: &CompressorParams) -> State {
        State { reduction_db: 0.0, last_makeup_db: params.makeup_db.load() }
    }

    /// Обрабатывает блок (стерео, по очереди). Возвращает наибольшее сжатие за блок, дБ.
    pub fn process(&mut self, buf: &mut [f32], p: &CompressorParams) -> f32 {
        let threshold = p.threshold_db.load();
        let slope = 1.0 - 1.0 / p.ratio.load().max(1.0);
        let attack = smoothing(p.attack_ms.load());
        let release = smoothing(p.release_ms.load());
        let makeup = p.makeup_db.load();
        let (frames, _) = buf.as_chunks_mut::<CHANNELS>();
        let n = frames.len() as f32;
        let mut most = 0.0f32;
        for (i, frame) in frames.iter_mut().enumerate() {
            let level = frame.iter().fold(0.0f32, |m, s| m.max(s.abs())).max(FLOOR);
            let target = static_reduction(20.0 * level.log10() - threshold, slope);
            let coef = if target > self.reduction_db { attack } else { release };
            self.reduction_db = target + coef * (self.reduction_db - target);
            most = most.max(self.reduction_db);
            let makeup_now = self.last_makeup_db + (makeup - self.last_makeup_db) * (i + 1) as f32 / n;
            let gain = db_to_gain(makeup_now - self.reduction_db);
            frame.iter_mut().for_each(|s| *s *= gain);
        }
        self.last_makeup_db = makeup;
        most
    }
}

/// Сколько дБ убрать при превышении порога на `over` дБ (мягкое колено).
fn static_reduction(over: f32, slope: f32) -> f32 {
    if 2.0 * over < -KNEE_DB {
        0.0
    } else if 2.0 * over.abs() <= KNEE_DB {
        slope * (over + KNEE_DB / 2.0).powi(2) / (2.0 * KNEE_DB)
    } else {
        slope * over
    }
}

/// Коэффициент сглаживания за один кадр для времени `ms` (за это время — около 63 % пути).
fn smoothing(ms: f32) -> f32 {
    (-1.0 / (ms.max(0.01) * 0.001 * RATE as f32)).exp()
}

fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AtomicF32, BLOCK_SAMPLES};

    fn params(threshold: f32, ratio: f32, attack: f32, release: f32, makeup: f32) -> CompressorParams {
        CompressorParams {
            threshold_db: AtomicF32::new(threshold),
            ratio: AtomicF32::new(ratio),
            attack_ms: AtomicF32::new(attack),
            release_ms: AtomicF32::new(release),
            makeup_db: AtomicF32::new(makeup),
        }
    }

    fn db(x: f32) -> f32 {
        20.0 * x.log10()
    }

    /// Гоняем постоянный по громкости сигнал, пока сжатие не установится; возвращаем выходной пик.
    fn settled_peak(p: &CompressorParams, amplitude: f32) -> f32 {
        let mut st = State::new(p);
        let mut peak = 0.0;
        for b in 0..400 {
            let mut buf = [amplitude; BLOCK_SAMPLES];
            st.process(&mut buf, p);
            if b == 399 {
                peak = buf.iter().fold(0.0f32, |m, s| m.max(s.abs()));
            }
        }
        peak
    }

    #[test]
    fn quiet_sound_only_gets_makeup() {
        // −40 дБ при пороге −20: компрессор не трогает, усиление +6 дБ.
        let p = params(-20.0, 4.0, 5.0, 100.0, 6.0);
        let out = settled_peak(&p, 0.01);
        assert!((db(out) - (-40.0 + 6.0)).abs() < 0.05, "{}", db(out));
    }

    #[test]
    fn loud_sound_is_compressed_by_ratio() {
        // −4 дБ при пороге −20 и силе 4: превышение 16 дБ → остаётся 4 дБ, итог −16 дБ.
        let p = params(-20.0, 4.0, 1.0, 50.0, 0.0);
        let out = settled_peak(&p, db_to_gain(-4.0));
        assert!((db(out) - (-16.0)).abs() < 0.1, "{}", db(out));
    }

    #[test]
    fn ratio_one_changes_nothing() {
        let p = params(-30.0, 1.0, 1.0, 50.0, 0.0);
        let out = settled_peak(&p, 0.5);
        assert!((out - 0.5).abs() < 1e-4);
    }

    #[test]
    fn knee_is_continuous() {
        let slope = 0.75;
        for edge in [-KNEE_DB / 2.0, KNEE_DB / 2.0] {
            let a = static_reduction(edge - 1e-3, slope);
            let b = static_reduction(edge + 1e-3, slope);
            assert!((a - b).abs() < 1e-2, "{edge}: {a} vs {b}");
        }
    }

    #[test]
    fn attack_is_faster_than_release() {
        let p = params(-20.0, 8.0, 2.0, 500.0, 0.0);
        let mut st = State::new(&p);
        // Громкий блок: за 5 мс атаки 2 мс сжатие почти установилось.
        let mut loud = [0.9f32; BLOCK_SAMPLES];
        let r_loud = st.process(&mut loud, &p);
        assert!(r_loud > 10.0, "{r_loud}");
        // Тишина: за 5 мс при восстановлении 500 мс сжатие почти не ушло.
        let mut quiet = [0.0f32; BLOCK_SAMPLES];
        st.process(&mut quiet, &p);
        assert!(st.reduction_db > r_loud * 0.9, "{}", st.reduction_db);
    }

    #[test]
    fn makeup_change_has_no_jump() {
        let p = params(-20.0, 1.0, 1.0, 50.0, 0.0);
        let mut st = State::new(&p);
        let mut buf = [0.1f32; BLOCK_SAMPLES];
        st.process(&mut buf, &p);
        p.makeup_db.store(12.0);
        let mut buf = [0.1f32; BLOCK_SAMPLES];
        st.process(&mut buf, &p);
        // Внутри блока соседние кадры отличаются чуть-чуть, а к концу — полные +12 дБ.
        let step = buf.chunks(2).collect::<Vec<_>>().windows(2).fold(0.0f32, |m, w| m.max((w[1][0] - w[0][0]).abs()));
        assert!(step < 0.01, "{step}");
        assert!((db(buf[BLOCK_SAMPLES - 1] / 0.1) - 12.0).abs() < 0.01);
    }
}
