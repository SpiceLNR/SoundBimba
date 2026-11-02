// SPDX-License-Identifier: AGPL-3.0-or-later
//! Морфер голоса: пять эффектов по очереди — высота (с «дрожанием»), «робот», «рация», эхо. Готовые голоса
//! (интерфейс) — это сочетания их силы; все нули — голос проходит без изменений.
//!
//! - Высота: сдвиг на полутона двумя «скользящими» отводами линии задержки с плавной сменой
//!   (окно 40 мс, задержка около 20 мс). Пока сдвиг 0 — сигнал идёт мимо, без задержки.
//! - Дрожание: высота колеблется 7 раз в секунду, до ±2 полутонов («пришелец»).
//! - Робот: кольцевая модуляция частотой 55 Гц (металлический «далек»).
//! - Рация: полоса 450–2800 Гц и лёгкий перегруз.
//! - Эхо: повтор через 170 мс с затуханием.
//!
//! Сила каждого эффекта меняется плавно внутри блока — без щелчков. Буферы выделяются при
//! компиляции графа; в обработке — только арифметика.

use std::f32::consts::{PI, TAU};

use crate::{AtomicF32, CHANNELS, RATE};

/// Окно сдвига высоты: 40 мс.
const PITCH_WINDOW: f32 = RATE as f32 * 0.040;
/// Размер линии задержки сдвига (степень двойки, больше окна).
const PITCH_BUF: usize = 4096;
const ROBOT_HZ: f32 = 55.0;
const WOBBLE_HZ: f32 = 7.0;
/// Размах дрожания при силе 1: ±2 полутона.
const WOBBLE_SEMITONES: f32 = 2.0;
const ECHO_DELAY: usize = RATE as usize * 170 / 1000;
const ECHO_BUF: usize = 16384;
const ECHO_FEEDBACK: f32 = 0.4;
const ECHO_LEVEL: f32 = 0.6;
const RADIO_DRIVE: f32 = 2.5;

/// Сила эффектов (пишет интерфейс). Пределы — `graph::MORPHER_RANGES`.
#[derive(Debug)]
pub struct MorpherParams {
    /// Сдвиг высоты, полутона (−12…12).
    pub pitch: AtomicF32,
    pub robot: AtomicF32,
    pub radio: AtomicF32,
    pub echo: AtomicF32,
    /// Дрожание высоты 0…1.
    pub wobble: AtomicF32,
}

impl Default for MorpherParams {
    fn default() -> Self {
        MorpherParams {
            pitch: AtomicF32::new(0.0),
            robot: AtomicF32::new(0.0),
            radio: AtomicF32::new(0.0),
            echo: AtomicF32::new(0.0),
            wobble: AtomicF32::new(0.0),
        }
    }
}

/// Двухполюсный фильтр (RBJ), состояние на канал.
#[derive(Clone, Copy, Default)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: [f32; CHANNELS],
    z2: [f32; CHANNELS],
}

impl Biquad {
    fn new(high_pass: bool, freq: f32) -> Biquad {
        let w = TAU * freq / RATE as f32;
        let alpha = w.sin() / (2.0 * std::f32::consts::FRAC_1_SQRT_2);
        let cos = w.cos();
        let a0 = 1.0 + alpha;
        let (b0, b1) = if high_pass { ((1.0 + cos) / 2.0, -(1.0 + cos)) } else { ((1.0 - cos) / 2.0, 1.0 - cos) };
        Biquad { b0: b0 / a0, b1: b1 / a0, b2: b0 / a0, a1: -2.0 * cos / a0, a2: (1.0 - alpha) / a0, ..Default::default() }
    }

    fn run(&mut self, ch: usize, x: f32) -> f32 {
        let y = self.b0 * x + self.z1[ch];
        self.z1[ch] = self.b1 * x - self.a1 * y + self.z2[ch];
        self.z2[ch] = self.b2 * x - self.a2 * y;
        y
    }
}

/// Плавная сила эффекта: от прошлого блока к нынешнему значению.
#[derive(Default)]
struct Ramp {
    last: f32,
}

impl Ramp {
    fn at(&self, to: f32, i: usize, n: usize) -> f32 {
        self.last + (to - self.last) * (i + 1) as f32 / n as f32
    }
}

pub struct State {
    pitch_buf: Box<[[f32; PITCH_BUF]; CHANNELS]>,
    pitch_write: usize,
    /// Задержка первого отвода, отсчётов (второй — на полокна дальше).
    pitch_delay: f32,
    pitch_mix: Ramp,
    robot_phase: f32,
    robot: Ramp,
    wobble_phase: f32,
    wobble: Ramp,
    high: Biquad,
    low: Biquad,
    radio: Ramp,
    echo_buf: Box<[[f32; ECHO_BUF]; CHANNELS]>,
    echo_write: usize,
    echo: Ramp,
}

impl Default for State {
    fn default() -> Self {
        State {
            pitch_buf: Box::new([[0.0; PITCH_BUF]; CHANNELS]),
            pitch_write: 0,
            pitch_delay: 0.0,
            pitch_mix: Ramp::default(),
            robot_phase: 0.0,
            robot: Ramp::default(),
            wobble_phase: 0.0,
            wobble: Ramp::default(),
            high: Biquad::new(true, 450.0),
            low: Biquad::new(false, 2800.0),
            radio: Ramp::default(),
            echo_buf: Box::new([[0.0; ECHO_BUF]; CHANNELS]),
            echo_write: 0,
            echo: Ramp::default(),
        }
    }
}

/// Плавное окно отвода: 0 на краях (там отвод «перескакивает»), сумма двух отводов — 1.
fn tap_weight(delay: f32) -> f32 {
    let s = (PI * delay / PITCH_WINDOW).sin();
    s * s
}

impl State {
    fn pitch_tap(&self, ch: usize, delay: f32) -> f32 {
        let pos = self.pitch_write as f32 - delay;
        let base = pos.floor();
        let frac = pos - base;
        let i = base as isize;
        let buf = &self.pitch_buf[ch];
        let at = |k: isize| buf[(k.rem_euclid(PITCH_BUF as isize)) as usize];
        at(i) + (at(i + 1) - at(i)) * frac
    }

    /// Обрабатывает блок (стерео, по очереди) на месте.
    pub fn process(&mut self, buf: &mut [f32], p: &MorpherParams) {
        let semitones = p.pitch.load().clamp(-12.0, 12.0);
        let wobble = p.wobble.load().clamp(0.0, 1.0);
        let pitch_on = if semitones.abs() > 0.01 || wobble > 0.0 { 1.0 } else { 0.0 };
        let robot = p.robot.load().clamp(0.0, 1.0);
        let radio = p.radio.load().clamp(0.0, 1.0);
        let echo = p.echo.load().clamp(0.0, 1.0);
        let frames = buf.len() / CHANNELS;
        let robot_step = TAU * ROBOT_HZ / RATE as f32;
        let wobble_step = TAU * WOBBLE_HZ / RATE as f32;
        for f in 0..frames {
            let mix = self.pitch_mix.at(pitch_on, f, frames);
            let robot_now = self.robot.at(robot, f, frames);
            let radio_now = self.radio.at(radio, f, frames);
            let echo_now = self.echo.at(echo, f, frames);
            let wobble_now = self.wobble.at(wobble, f, frames);
            let semis_now = semitones + wobble_now * WOBBLE_SEMITONES * self.wobble_phase.sin();
            let ratio = 2f32.powf(semis_now / 12.0);
            let d0 = self.pitch_delay;
            let d1 = (d0 + PITCH_WINDOW / 2.0) % PITCH_WINDOW;
            let carrier = 1.0 - robot_now + robot_now * self.robot_phase.sin();
            for ch in 0..CHANNELS {
                let x = buf[f * CHANNELS + ch];
                // Высота (линия задержки пишется всегда — чтобы включение было без щелчка).
                self.pitch_buf[ch][self.pitch_write] = x;
                let mut y = x;
                if mix > 0.0 {
                    let shifted = self.pitch_tap(ch, d0) * tap_weight(d0) + self.pitch_tap(ch, d1) * tap_weight(d1);
                    y = x * (1.0 - mix) + shifted * mix;
                }
                // Робот.
                y *= carrier;
                // Рация: фильтры работают всегда (их состояние не прыгает при включении).
                let band = self.low.run(ch, self.high.run(ch, y));
                let driven = (band * RADIO_DRIVE).tanh() / RADIO_DRIVE.tanh();
                y = y * (1.0 - radio_now) + driven * radio_now;
                // Эхо.
                let read = (self.echo_write + ECHO_BUF - ECHO_DELAY) % ECHO_BUF;
                let delayed = self.echo_buf[ch][read];
                self.echo_buf[ch][self.echo_write] = y + delayed * ECHO_FEEDBACK;
                y += delayed * ECHO_LEVEL * echo_now;
                buf[f * CHANNELS + ch] = if y.is_finite() { y } else { 0.0 };
            }
            self.pitch_write = (self.pitch_write + 1) % PITCH_BUF;
            self.echo_write = (self.echo_write + 1) % ECHO_BUF;
            self.pitch_delay = (self.pitch_delay + 1.0 - ratio).rem_euclid(PITCH_WINDOW);
            self.robot_phase = (self.robot_phase + robot_step) % TAU;
            self.wobble_phase = (self.wobble_phase + wobble_step) % TAU;
        }
        self.pitch_mix.last = pitch_on;
        self.robot.last = robot;
        self.radio.last = radio;
        self.echo.last = echo;
        self.wobble.last = wobble;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BLOCK_SAMPLES;

    fn params(pitch: f32, robot: f32, radio: f32, echo: f32) -> MorpherParams {
        MorpherParams {
            pitch: AtomicF32::new(pitch),
            robot: AtomicF32::new(robot),
            radio: AtomicF32::new(radio),
            echo: AtomicF32::new(echo),
            wobble: AtomicF32::new(0.0),
        }
    }

    fn sine(freq: f32, seconds: f32) -> Vec<f32> {
        let n = (RATE as f32 * seconds) as usize;
        (0..n).flat_map(|i| {
            let v = 0.5 * (TAU * freq * i as f32 / RATE as f32).sin();
            [v, v]
        })
        .collect()
    }

    fn run(p: &MorpherParams, input: &[f32]) -> Vec<f32> {
        let mut st = State::default();
        let mut out = input.to_vec();
        for chunk in out.chunks_mut(BLOCK_SAMPLES) {
            st.process(chunk, p);
        }
        out
    }

    /// Частота по переходам через ноль (левый канал, после первых 0,2 с).
    fn freq(x: &[f32]) -> f32 {
        let left: Vec<f32> = x.iter().step_by(2).skip(RATE as usize / 5).copied().collect();
        let crossings = left.windows(2).filter(|w| (w[0] < 0.0) != (w[1] < 0.0)).count();
        crossings as f32 / 2.0 / (left.len() as f32 / RATE as f32)
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|s| s * s).sum::<f32>() / x.len() as f32).sqrt()
    }

    #[test]
    fn normal_voice_passes_unchanged() {
        let input = sine(300.0, 0.5);
        let out = run(&params(0.0, 0.0, 0.0, 0.0), &input);
        // Эхо выключено, но линия задержки пишется: выход совпадает со входом.
        assert!(input.iter().zip(&out).all(|(a, b)| (a - b).abs() < 1e-6));
    }

    #[test]
    fn pitch_moves_frequency_by_semitones() {
        for (semi, expect) in [(12.0, 800.0), (-12.0, 200.0), (7.0, 400.0 * 2f32.powf(7.0 / 12.0))] {
            let out = run(&params(semi, 0.0, 0.0, 0.0), &sine(400.0, 1.0));
            let got = freq(&out);
            assert!((got - expect).abs() / expect < 0.03, "{semi}: {got} vs {expect}");
            // Громкость примерно та же (окна отводов в сумме дают 1).
            let ratio = rms(&out[RATE as usize..]) / rms(&sine(400.0, 1.0)[RATE as usize..]);
            assert!((0.7..1.3).contains(&ratio), "{semi}: уровень ×{ratio}");
        }
    }

    #[test]
    fn wobble_makes_pitch_swing_around_the_center() {
        let p = params(0.0, 0.0, 0.0, 0.0);
        p.wobble.store(1.0);
        let out = run(&p, &sine(400.0, 2.0));
        // В среднем частота та же, а в отрезках по 20 мс — то выше, то ниже (до ±2 полутонов).
        assert!((freq(&out) - 400.0).abs() < 8.0, "{}", freq(&out));
        let seg = RATE as usize / 50 * CHANNELS;
        let freqs: Vec<f32> = out[RATE as usize..].chunks(seg).map(|c| {
            let l: Vec<f32> = c.iter().step_by(2).copied().collect();
            l.windows(2).filter(|w| (w[0] < 0.0) != (w[1] < 0.0)).count() as f32 / 2.0 / (l.len() as f32 / RATE as f32)
        }).collect();
        let (lo, hi) = freqs.iter().fold((f32::MAX, 0.0f32), |(a, b), &f| (a.min(f), b.max(f)));
        assert!(lo < 380.0 && hi > 420.0 && hi < 470.0, "{lo}..{hi}");
    }

    #[test]
    fn radio_cuts_lows_and_highs() {
        let p = params(0.0, 0.0, 1.0, 0.0);
        let pass = rms(&run(&p, &sine(1200.0, 0.5))[RATE as usize / 2..]);
        let low = rms(&run(&p, &sine(80.0, 0.5))[RATE as usize / 2..]);
        let high = rms(&run(&p, &sine(9000.0, 0.5))[RATE as usize / 2..]);
        assert!(low < pass * 0.2 && high < pass * 0.2, "низ {low}, середина {pass}, верх {high}");
    }

    #[test]
    fn robot_modulates_the_level() {
        let out = run(&params(0.0, 1.0, 0.0, 0.0), &sine(1000.0, 0.5));
        // Огибающая «дышит» с частотой 55 Гц: в одних местах почти тишина, в других — полный уровень.
        let win = (RATE as usize / 1000) * CHANNELS; // 1 мс
        let levels: Vec<f32> = out.chunks(win).map(rms).collect();
        let (lo, hi) = levels.iter().fold((f32::MAX, 0.0f32), |(a, b), &l| (a.min(l), b.max(l)));
        assert!(lo < hi * 0.2, "{lo} {hi}");
    }

    #[test]
    fn echo_repeats_after_170_ms() {
        // Короткий щелчок, затем тишина: повтор должен прийти через 170 мс.
        let mut input = vec![0.0f32; RATE as usize * CHANNELS / 2];
        input[0] = 1.0;
        input[1] = 1.0;
        let out = run(&params(0.0, 0.0, 0.0, 1.0), &input);
        let at = ECHO_DELAY * CHANNELS;
        assert!((out[at] - ECHO_LEVEL).abs() < 1e-4, "{}", out[at]);
        assert!(out[at - 2].abs() < 1e-6 && out[at + 2].abs() < 1e-6);
    }

    #[test]
    fn switching_voices_has_no_jumps() {
        let input = sine(200.0, 1.0);
        let mut st = State::default();
        let p = params(0.0, 0.0, 0.0, 0.0);
        let mut out = input.clone();
        let blocks: Vec<&mut [f32]> = out.chunks_mut(BLOCK_SAMPLES).collect();
        for (i, chunk) in blocks.into_iter().enumerate() {
            // Каждые 10 блоков — другой голос.
            let v = [(0.0, 0.0, 0.0, 0.0), (-7.0, 0.0, 0.0, 0.0), (0.0, 1.0, 0.0, 0.0), (3.0, 0.5, 1.0, 0.5)][(i / 10) % 4];
            p.pitch.store(v.0);
            p.robot.store(v.1);
            p.radio.store(v.2);
            p.echo.store(v.3);
            st.process(chunk, &p);
        }
        // У синуса 200 Гц и уровня 0,5 соседние отсчёты отличаются не больше чем на ~0,013;
        // скачок при смене голоса был бы заметно больше.
        let step = out.chunks(2).collect::<Vec<_>>().windows(2).fold(0.0f32, |m, w| m.max((w[1][0] - w[0][0]).abs()));
        assert!(step < 0.06, "скачок {step}");
    }
}
