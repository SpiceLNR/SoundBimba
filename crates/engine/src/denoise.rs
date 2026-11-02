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

use nnnoiseless::DenoiseState;

use crate::{AtomicF32, CHANNELS};

const FRAME: usize = DenoiseState::FRAME_SIZE;
/// Задержка выхода: кадр набора + кадр RNNoise.
pub const LATENCY: usize = 2 * FRAME;
/// RNNoise ждёт отсчёты в масштабе 16-бит.
const SCALE: f32 = 32768.0;

pub struct State {
    rnn: Box<DenoiseState<'static>>,
    input: [f32; FRAME],
    output: [f32; FRAME],
    pos: usize,
    /// Исходный звук, задержанный на `LATENCY`, — для смешивания по «силе».
    dry: Box<[f32; LATENCY]>,
    dry_pos: usize,
    last_amount: f32,
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
    pub fn process(&mut self, buf: &mut [f32], amount: &AtomicF32) -> f32 {
        let amount = amount.load().clamp(0.0, 1.0);
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
                voice = voice.max(self.rnn.process_frame(&mut self.output, &input));
                self.pos = 0;
            }
            let a = self.last_amount + (amount - self.last_amount) * (f + 1) as f32 / frames as f32;
            let y = wet * a + dry * (1.0 - a);
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
        let mut st = State::default();
        st.warm_up();
        let a = AtomicF32::new(amount);
        let mut out = input.to_vec();
        let mut voice = 0.0f32;
        for chunk in out.chunks_mut(BLOCK_SAMPLES) {
            voice = voice.max(st.process(chunk, &a));
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
}

