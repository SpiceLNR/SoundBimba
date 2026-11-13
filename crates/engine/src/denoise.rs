// SPDX-License-Identifier: AGPL-3.0-or-later
//! Шумоподавление: RNNoise (библиотека nnnoiseless, BSD-3-Clause) — маленькая нейросеть, которая
//! оставляет голос и убирает фон (вентилятор, клавиатура, гул). Работает на процессоре.
//!
//! - Звук сводится в моно (узел — для голоса) и обрабатывается кадрами по 10 мс; задержка —
//!   кадр на набор и кадр внутри RNNoise, итого 20 мс.
//! - «Сила» смешивает очищенный звук с исходным (тоже задержанным на 20 мс — без эха).
//! - Память выделяется при создании. FFT библиотеки при первом вызове в потоке заводит свои
//!   буферы — поэтому `warm_up` вызывается в потоке движка до начала обработки (см. runner).
//! - Отдаёт вероятность голоса (0…1) — для индикатора «слышу голос».
//! - «Тишина между словами» (2026-11-14, по жалобе тестировщика: слышно клавиатуру, мышь, шум
//!   компьютера): пока RNNoise не слышит голоса, микрофон плавно приглушается (до −40 дБ при
//!   силе 1); голос — открывается за 2 мс. Решение о голосе приходит раньше, чем звучит этот
//!   кадр, — начало слова не съедается. После голоса держим открытым 250 мс (между словами).

use nnnoiseless::DenoiseState;

use crate::{AtomicF32, CHANNELS};

const FRAME: usize = DenoiseState::FRAME_SIZE;
/// Задержка выхода: кадр набора + кадр RNNoise.
pub const LATENCY: usize = 2 * FRAME;
/// RNNoise ждёт отсчёты в масштабе 16-бит.
const SCALE: f32 = 32768.0;
/// «Тишина между словами»: голос — вероятность от RNNoise не меньше GATE_VOICE; держим открытым
/// столько кадров по 10 мс; глубина при силе 1; открытие за 2 мс, закрытие за 150 мс (в отсчётах).
const GATE_VOICE: f32 = 0.5;
const GATE_HOLD_FRAMES: u32 = 25;
const GATE_DEPTH_DB: f32 = 40.0;
const GATE_ATTACK: f32 = 96.0;
const GATE_RELEASE: f32 = 7200.0;

pub struct State {
    rnn: Box<DenoiseState<'static>>,
    input: [f32; FRAME],
    output: [f32; FRAME],
    pos: usize,
    /// Исходный звук, задержанный на `LATENCY`, — для смешивания по «силе».
    dry: Box<[f32; LATENCY]>,
    dry_pos: usize,
    last_amount: f32,
    /// Усиление «ворот» (1 — открыто) и сколько кадров ещё держать открытыми.
    gate_gain: f32,
    gate_hold: u32,
}

impl Default for State {
    fn default() -> Self {
        State {
            rnn: DenoiseState::new(),
            input: [0.0; FRAME],
            output: [0.0; FRAME],
            pos: 0,
            dry: Box::new([0.0; LATENCY]),
            dry_pos: 0,
            last_amount: 1.0,
            gate_gain: 1.0,
            gate_hold: 0,
        }
    }
}

impl State {
    /// Первый вызов в потоке движка (до реального времени): FFT заводит свои буферы здесь.
    pub fn warm_up(&mut self) {
        let mut out = [0.0; FRAME];
        self.rnn.process_frame(&mut out, &[0.0; FRAME]);
    }

    /// Обрабатывает блок на месте. `amount` — сила 0…1. Возвращает вероятность голоса (наибольшую
    /// за кадры, закончившиеся в этом блоке; 0 — кадр не закончился).
    pub fn process(&mut self, buf: &mut [f32], amount: &AtomicF32, gate: &AtomicF32) -> f32 {
        let amount = amount.load().clamp(0.0, 1.0);
        let gate = gate.load().clamp(0.0, 1.0);
        // Насколько закрываются ворота: 0 — не закрываются, 1 — до −40 дБ.
        let floor = 10f32.powf(-GATE_DEPTH_DB * gate / 20.0);
        let frames = buf.len() / CHANNELS;
        let mut voice = 0.0f32;
        for (f, frame) in buf.as_chunks_mut::<CHANNELS>().0.iter_mut().enumerate() {
            let x = frame.iter().sum::<f32>() / CHANNELS as f32;
            let dry = std::mem::replace(&mut self.dry[self.dry_pos], x);
            self.dry_pos = (self.dry_pos + 1) % LATENCY;
            self.input[self.pos] = x * SCALE;
            let wet = self.output[self.pos] / SCALE;
            self.pos += 1;
            if self.pos == FRAME {
                let input = self.input;
                let p = self.rnn.process_frame(&mut self.output, &input);
                voice = voice.max(p);
                self.gate_hold = if p >= GATE_VOICE { GATE_HOLD_FRAMES } else { self.gate_hold.saturating_sub(1) };
                self.pos = 0;
            }
            let a = self.last_amount + (amount - self.last_amount) * (f + 1) as f32 / frames as f32;
            // Ворота: открываем быстро, закрываем плавно.
            let target = if self.gate_hold > 0 || gate == 0.0 { 1.0 } else { floor };
            self.gate_gain = if target > self.gate_gain {
                (self.gate_gain + (1.0 - floor) / GATE_ATTACK).min(target)
            } else {
                (self.gate_gain - (1.0 - floor) / GATE_RELEASE).max(target)
            };
            let y = (wet * a + dry * (1.0 - a)) * self.gate_gain;
            frame.fill(if y.is_finite() { y } else { 0.0 });
        }
        self.last_amount = amount;
        voice
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BLOCK_SAMPLES, RATE};
    use std::f32::consts::TAU;

    fn run(amount: f32, input: &[f32]) -> (Vec<f32>, f32) {
        run_gate(amount, 0.0, input)
    }

    fn run_gate(amount: f32, gate: f32, input: &[f32]) -> (Vec<f32>, f32) {
        let mut st = State::default();
        st.warm_up();
        let a = AtomicF32::new(amount);
        let g = AtomicF32::new(gate);
        let mut out = input.to_vec();
        let mut voice = 0.0f32;
        for chunk in out.chunks_mut(BLOCK_SAMPLES) {
            voice = voice.max(st.process(chunk, &a, &g));
        }
        (out, voice)
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|s| s * s).sum::<f32>() / x.len() as f32).sqrt()
    }

    /// Гул, как от вентилятора: шум, сглаженный в низкие частоты (повторяемый генератор).
    /// Ровное белое шипение RNNoise почти не трогает (оно похоже на «с», «ш» в речи) — замер
    /// 2026-11-03: гул −52…59 дБ за 3 с, белый шум лишь −3…5 дБ.
    fn fan_noise(seconds: f32, level: f32) -> Vec<f32> {
        let mut seed = 12345u32;
        let mut low = 0.0f32;
        let n = (RATE as f32 * seconds) as usize;
        (0..n)
            .flat_map(|_| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let white = (seed >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0;
                low += (white - low) * 0.05;
                let v = level * 4.0 * low;
                [v, v]
            })
            .collect()
    }

    #[test]
    fn fan_noise_is_removed() {
        // Гул без голоса: после того как сеть «освоится», остаётся во много раз тише.
        let input = fan_noise(3.0, 0.05);
        let (out, _) = run(1.0, &input);
        let tail = RATE as usize * CHANNELS;
        let ratio = rms(&out[tail..]) / rms(&input[tail..]);
        assert!(ratio < 0.05, "шум ослаблен лишь до ×{ratio}");
    }

    #[test]
    fn zero_amount_is_the_original_delayed_by_20_ms() {
        let input: Vec<f32> = (0..RATE as usize).flat_map(|i| {
            let v = 0.3 * (TAU * 300.0 * i as f32 / RATE as f32).sin();
            [v, v]
        }).collect();
        let (out, _) = run(0.0, &input);
        for i in (LATENCY..RATE as usize).step_by(97) {
            assert!((out[i * CHANNELS] - input[(i - LATENCY) * CHANNELS]).abs() < 1e-5);
        }
    }

    #[test]
    fn silence_stays_silent_and_finite() {
        let (out, voice) = run(1.0, &vec![0.0; RATE as usize * CHANNELS]);
        assert!(out.iter().all(|s| s.is_finite() && s.abs() < 1e-4));
        assert!(voice < 0.5);
    }

    /// Щелчки клавиатуры в тишине (голоса нет): «тишина между словами» их глушит.
    #[test]
    fn gate_silences_clicks_between_words() {
        let mut input = vec![0.0f32; RATE as usize * 2 * CHANNELS];
        let mut seed = 5u32;
        for k in 0..12 {
            let at = (RATE as usize / 8 + k * RATE as usize / 7) * CHANNELS;
            for j in 0..96 {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let v = ((seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 0.4 * (1.0 - j as f32 / 96.0);
                input[at + j * CHANNELS] = v;
                input[at + j * CHANNELS + 1] = v;
            }
        }
        let tail = RATE as usize / 2 * CHANNELS;
        let (open, _) = run_gate(0.0, 0.0, &input);
        let (closed, _) = run_gate(0.0, 1.0, &input);
        let db = 20.0 * (rms(&closed[tail..]) / rms(&open[tail..])).log10();
        assert!(db < -30.0, "щелчки тише лишь на {db} дБ");
    }

    #[test]
    fn gate_off_changes_nothing() {
        let input = fan_noise(1.0, 0.05);
        let (a, _) = run_gate(1.0, 0.0, &input);
        let (b, _) = run(1.0, &input);
        assert_eq!(a, b);
    }

    /// Ручная проверка на записи: SB_WAV (48 кГц, моно, float32), SB_OUT — папка. Пишет запись через
    /// шумодав с «тишиной между словами» 0 / 0,7 / 1 и печатает громкость пауз и речи.
    #[test]
    #[ignore = "ручная проверка на записи"]
    fn denoise_recording() {
        let b = std::fs::read(std::env::var("SB_WAV").expect("SB_WAV")).unwrap();
        let mut i = 12;
        let mut mono = Vec::new();
        while i + 8 <= b.len() {
            let len = u32::from_le_bytes(b[i + 4..i + 8].try_into().unwrap()) as usize;
            if &b[i..i + 4] == b"data" {
                mono = b[i + 8..i + 8 + len].chunks(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect();
                break;
            }
            i += 8 + len;
        }
        let dir = std::env::var("SB_OUT").expect("SB_OUT");
        let stereo: Vec<f32> = mono.iter().flat_map(|&v| [v, v]).collect();
        // Куски по 20 мс: громкие (речь) и тихие (паузы) — по исходнику.
        let win = 960;
        let levels: Vec<f32> = mono.chunks(win).map(rms).collect();
        let mut sorted = levels.clone();
        sorted.sort_by(f32::total_cmp);
        let (quiet, loud) = (sorted[sorted.len() / 5], sorted[sorted.len() * 4 / 5]);
        for (name, gate) in [("gate0", 0.0f32), ("gate07", 0.7), ("gate1", 1.0)] {
            let (out, _) = run_gate(1.0, gate, &stereo);
            let left: Vec<f32> = out.iter().step_by(2).copied().collect();
            let lv = |pick: &dyn Fn(f32) -> bool| {
                let xs: Vec<f32> = levels.iter().enumerate().filter(|(_, l)| pick(**l)).filter_map(|(k, _)| {
                    let from = k * win + LATENCY;
                    (from + win <= left.len()).then(|| rms(&left[from..from + win]))
                }).collect();
                20.0 * (xs.iter().sum::<f32>() / xs.len().max(1) as f32 + 1e-9).log10()
            };
            println!("{name}: паузы {:.1} дБ, речь {:.1} дБ", lv(&|l| l <= quiet), lv(&|l| l >= loud));
            let mut w = Vec::new();
            let data = (left.len() * 4) as u32;
            w.extend_from_slice(b"RIFF");
            w.extend_from_slice(&(36 + data).to_le_bytes());
            w.extend_from_slice(b"WAVEfmt ");
            for v in [16u32.to_le_bytes(), [3, 0, 1, 0], RATE.to_le_bytes(), (RATE * 4).to_le_bytes(), [4, 0, 32, 0]] {
                w.extend_from_slice(&v);
            }
            w.extend_from_slice(b"data");
            w.extend_from_slice(&data.to_le_bytes());
            for v in &left {
                w.extend_from_slice(&v.to_le_bytes());
            }
            std::fs::write(format!("{dir}/{name}.wav"), w).unwrap();
        }
        println!("исходник: паузы {:.1} дБ, речь {:.1} дБ", 20.0 * quiet.log10(), 20.0 * loud.log10());
    }
}

