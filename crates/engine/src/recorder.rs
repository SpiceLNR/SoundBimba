// SPDX-License-Identifier: AGPL-3.0-or-later
//! «Диктофон»: поток звука отдаёт отсчёты записи через кольцевой буфер (без ожидания и без
//! выделения памяти), на диск пишет приложение в своём потоке. Если писатель не успел и буфер
//! полон — пропадает кусок записи, но звук в наушниках и в виртуальном микрофоне не ждёт никогда.
//!
//! В буфере: метка начала — пара (NaN, номер записи), затем отсчёты стерео. Метку движок ставит,
//! когда начинает отдавать запись с этим номером: и в начале записи, и после перезапуска звука
//! (новый граф — новый буфер). Писатель по номеру видит: та же запись продолжается или новая.

use crate::NodeShared;

/// Ёмкость буфера записи: 2 с (писатель забирает каждые ~20 мс).
pub const TAP_SECONDS: usize = 2;
const TAP_SAMPLES: usize = crate::RATE as usize * crate::CHANNELS * TAP_SECONDS;

/// Сторона движка.
pub struct RecorderTap {
    prod: rtrb::Producer<f32>,
}

/// Сторона писателя.
pub struct RecorderFeed {
    cons: rtrb::Consumer<f32>,
    scratch: Vec<f32>,
}

/// Новый буфер записи для одного «Диктофона» (создаётся вне потока звука).
pub fn tap() -> (RecorderTap, RecorderFeed) {
    let (prod, cons) = rtrb::RingBuffer::new(TAP_SAMPLES);
    (RecorderTap { prod }, RecorderFeed { cons, scratch: Vec::with_capacity(TAP_SAMPLES) })
}

/// Что пришло из буфера.
#[derive(Debug, PartialEq)]
pub enum Piece<'a> {
    /// Отсюда — запись с этим номером.
    Start(u32),
    /// Отсчёты стерео (по очереди левый, правый).
    Samples(&'a [f32]),
}

impl RecorderFeed {
    /// Забирает всё, что есть в буфере, по порядку.
    pub fn drain(&mut self, mut f: impl FnMut(Piece<'_>)) {
        let n = self.cons.slots();
        if n == 0 {
            return;
        }
        let Ok(chunk) = self.cons.read_chunk(n) else { return };
        let (a, b) = chunk.as_slices();
        self.scratch.clear();
        self.scratch.extend_from_slice(a);
        self.scratch.extend_from_slice(b);
        chunk.commit_all();
        let mut from = 0;
        let mut i = 0;
        while i < self.scratch.len() {
            if self.scratch[i].is_nan() {
                if from < i {
                    f(Piece::Samples(&self.scratch[from..i]));
                }
                // Метка кладётся парой за раз — номер всегда рядом.
                let session = self.scratch.get(i + 1).copied().unwrap_or(0.0) as u32;
                f(Piece::Start(session));
                i += 2;
                from = i;
            } else {
                i += 1;
            }
        }
        if from < self.scratch.len() {
            f(Piece::Samples(&self.scratch[from..]));
        }
    }

    /// Движок с этим буфером остановлен (граф удалён) — больше ничего не придёт.
    pub fn is_abandoned(&self) -> bool {
        self.cons.is_abandoned()
    }
}

/// Один блок «Диктофона» в потоке звука: если идёт запись — отдать блок писателю. `marked` —
/// номер записи, для которой метка уже поставлена (0 — нет). Реальное время: без ожидания.
pub fn record(tap: Option<&mut RecorderTap>, marked: &mut u32, shared: &NodeShared, block: &[f32]) {
    let (on, session) = shared.recording();
    if !on {
        *marked = 0;
        return;
    }
    let Some(tap) = tap else { return };
    if *marked != session {
        match tap.prod.write_chunk_uninit(2) {
            Ok(chunk) => {
                chunk.fill_from_iter([f32::NAN, session as f32]);
                *marked = session;
            }
            Err(_) => {
                shared.add_rec_dropped(block.len());
                return;
            }
        }
    }
    match tap.prod.write_chunk_uninit(block.len()) {
        Ok(chunk) => {
            // NaN в звуке приняли бы за метку — такой отсчёт пишем тишиной.
            chunk.fill_from_iter(block.iter().map(|&v| if v.is_nan() { 0.0 } else { v }));
        }
        Err(_) => shared.add_rec_dropped(block.len()),
    }
}
