// SPDX-License-Identifier: AGPL-3.0-or-later
//! Морфер голоса: эффекты по очереди — высота (с «дрожанием»), тембр, «робот», «рация», рык, хор,
//! эхо, зал. Готовые голоса (интерфейс) — это сочетания их силы; все нули — голос проходит без
//! изменений. Тембр, рык, хор и зал добавлены 2026-11-12; выключенные, они не трогают звук —
//! старые голоса звучат ровно как раньше (проверка — копия прежнего морфера, `morpher_legacy.rs`).
//!
//! - Высота: сдвиг на полутона двумя «скользящими» отводами линии задержки с плавной сменой
//!   (окно 40 мс, задержка около 20 мс). Пока сдвиг 0 — сигнал идёт мимо, без задержки.
//! - Дрожание: высота колеблется 7 раз в секунду, до ±2 полутонов («пришелец»).
//! - Робот: кольцевая модуляция частотой 55 Гц (металлический «далек»).
//! - Рация: полоса 450–2800 Гц и лёгкий перегруз.
//! - Высота и тембр (2026-11-13, по примеру MorphVOX и AV Voice Changer): высота без смены окраски
//!   голоса и окраска (форманты) без смены высоты — по отдельности. PSOLA: зёрна по периоду
//!   голоса (YIN), ≈40 мс задержки, пока они изменены, моно. Шипящие по высоте не сдвигаются.
//!   (Первая версия — спектральная, фазовый вокодер — на живом голосе звучала «роботом».)
//!   Старая «высота» (`pitch`) — мультяшная: сдвигает голос целиком.
//! - Рык: перегруз с ограничением и «рычащая» модуляция 38 Гц, верха срезаны.
//! - Хор: три копии голоса с плавающими задержками 12–25 мс — «толпа» или многоголосое существо.
//! - Эхо: повтор через 170 мс с затуханием.
//! - Зал: реверберация (схема Freeverb: 8 гребенчатых и 4 всепропускающих фильтра) — пространство.
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

/// Высота и тембр (PSOLA). Кольца входа и выхода; самое низкое — 80 Гц (период 600 отсчётов),
/// самое высокое — 600 Гц. Зерно кладётся заранее (самое длинное — 600 / 0,63 отсчёта), задержка —
/// запас на зерно и два периода (зерно — смесь двух соседних): 2200 отсчётов ≈ 46 мс (пока высота
/// или тембр изменены).
const PSOLA_RING: usize = 8192;
const PSOLA_LOOKAHEAD: usize = 960;
const PSOLA_LATENCY: usize = 2200;
/// Поиск периода (YIN): окно сравнения, наименьший и наибольший период, как часто искать.
const YIN_WIN: usize = 1024;
const YIN_MIN_LAG: usize = RATE as usize / 600;
const YIN_MAX_LAG: usize = RATE as usize / 80;
/// Каждые 5 мс, с плавным переходом к новой оценке (ступеньки высоты — тоже дрожь).
const YIN_EVERY: usize = 240;
const PERIOD_SMOOTH: f64 = 0.5;
/// Яма глубже YIN_THRESHOLD — период найден; если и лучшая мельче YIN_VOICED — это шум.
const YIN_THRESHOLD: f32 = 0.15;
const YIN_VOICED: f32 = 0.3;
/// Если только что был голос — достаточно ямы мельче этого.
const YIN_KEEP_VOICED: f32 = 0.55;
/// Шаг зёрен для шума (с, ш, ф): 10 мс, высота не меняется.
const UNVOICED_STEP: f32 = 480.0;
/// Скорость перехода «голос ↔ шум» за отсчёт (полный переход — за 5 мс).
const VOICING_STEP: f32 = 1.0 / 240.0;
/// «Резкость» шипящих (энергия разности соседних отсчётов к энергии): выше — это «с», «ш», «ф»
/// (у «с» — 0,5–1,5; 0,45 ещё не задевает звонкие «з», «ж» и придыхание на записи автора).
const HISS_SHARPNESS: f32 = 0.45;
/// Подтягивать ли каждую метку к «толчку» связок (доля периода). На настоящем голосе автора
/// (2026-11-13) подтягивание каждый период давало дрожь — метки ложились чуть по-разному:
/// ровность голоса 0,64 против 0,70 без него (у исходного — 0,74). Метку ставим на толчок
/// только в начале (после шума или скачка), дальше — строго через период.
const SNAP: f64 = 0.0;

/// Для ручных опытов в тестах: SB_<имя> переопределяет число (в программе — всегда как задано).
fn knob(name: &str, value: f64) -> f64 {
    #[cfg(test)]
    if let Some(v) = std::env::var(format!("SB_{name}")).ok().and_then(|v| v.parse().ok()) {
        return v;
    }
    let _ = name;
    value
}

/// Рык: усиление перед перегрузом при силе 1, частота «рычания», срез верхов.
const GROWL_DRIVE: f32 = 14.0;
const GROWL_HZ: f32 = 38.0;
const GROWL_LOWPASS: f32 = 4500.0;

/// Хор: три голоса — средняя задержка (с), размах (с) и частота «плавания» (Гц).
const CHORUS_VOICES: [(f32, f32, f32); 3] = [(0.012, 0.003, 0.41), (0.018, 0.004, 0.63), (0.025, 0.005, 0.87)];
const CHORUS_BUF: usize = 4096;

/// Зал (Freeverb, длины для 44,1 кГц — пересчитываются на 48 кГц).
const COMBS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASSES: [usize; 4] = [556, 441, 341, 225];
/// Сдвиг длин правого канала — объём.
const STEREO_SPREAD: usize = 23;
const ROOM: f32 = 0.86;
const DAMP: f32 = 0.3;
const HALL_WET: f32 = 0.42;

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
    /// Тембр: сдвиг окраски голоса (формант), полутона (−8…8).
    pub formant: AtomicF32,
    /// Высота без смены окраски («естественная»), полутона (−12…12). Старая `pitch` —
    /// «мультяшная»: сдвигает голос целиком (бурундук, монстр).
    pub tone: AtomicF32,
    pub growl: AtomicF32,
    pub chorus: AtomicF32,
    pub hall: AtomicF32,
}

impl Default for MorpherParams {
    fn default() -> Self {
        MorpherParams {
            pitch: AtomicF32::new(0.0),
            robot: AtomicF32::new(0.0),
            radio: AtomicF32::new(0.0),
            echo: AtomicF32::new(0.0),
            wobble: AtomicF32::new(0.0),
            formant: AtomicF32::new(0.0),
            tone: AtomicF32::new(0.0),
            growl: AtomicF32::new(0.0),
            chorus: AtomicF32::new(0.0),
            hall: AtomicF32::new(0.0),
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

/// Высота и тембр по отдельности — как в «настоящих» морферах (MorphVOX, AV Voice Changer):
/// высота сдвигается, а окраска голоса (форманты) остаётся; тембр двигает окраску сам по себе.
///
/// Способ — PSOLA (так меняют высоту речи): находим период голоса (метод YIN), вырезаем из
/// голоса «зёрна» длиной два периода (с плавным окном) и раскладываем их чаще или реже — это
/// новая высота. Внутри зерна окраска голоса та же, поэтому она не меняется. Тембр — зерно
/// чуть сжимается или растягивается: окраска сдвигается, а высоту задаёт шаг раскладки.
///
/// Зёрна берутся строго через целое число периодов — голос не «склеивается» кусками (иначе —
/// жужжание с частотой кусков, «робот»). Шум (с, ш, ф, вдох) периода не имеет: его высоту не
/// меняем — зёрна идут с шагом 10 мс, только с тембром.
///
/// Все буферы — при создании; в обработке только арифметика.
struct Psola {
    /// Вход (моно), кольцо; `t` — номер текущего отсчёта.
    input: Box<[f32; PSOLA_RING]>,
    /// Сумма уложенных зёрен (выход), кольцо.
    acc: Box<[f32; PSOLA_RING]>,
    t: u64,
    /// Где (в отсчётах выхода) положить следующее зерно.
    next_mark: f64,
    /// Середина последнего взятого зерна (в отсчётах входа).
    in_center: f64,
    /// Период голоса, отсчётов; 0 — шум (периода нет).
    period: f32,
    /// Доля обработанного голоса в выходе: 1 — голос (зёрна), 0 — шум (идёт как есть).
    voicing: f32,
    since_detect: usize,
    /// Окно для поиска периода и разности YIN.
    detect: Box<[f32; YIN_WIN + YIN_MAX_LAG]>,
    diff: Box<[f32; YIN_MAX_LAG + 1]>,
    mix: Ramp,
}

impl Psola {
    fn new() -> Psola {
        Psola {
            input: Box::new([0.0; PSOLA_RING]),
            acc: Box::new([0.0; PSOLA_RING]),
            t: 0,
            next_mark: 0.0,
            in_center: 0.0,
            period: 0.0,
            voicing: 0.0,
            since_detect: 0,
            detect: Box::new([0.0; YIN_WIN + YIN_MAX_LAG]),
            diff: Box::new([0.0; YIN_MAX_LAG + 1]),
            mix: Ramp::default(),
        }
    }

    /// Сброс перед включением: старые зёрна не должны прозвучать.
    fn reset(&mut self) {
        self.input.fill(0.0);
        self.acc.fill(0.0);
        self.next_mark = self.t as f64 + PSOLA_LOOKAHEAD as f64;
        self.in_center = self.next_mark - PSOLA_LATENCY as f64;
        self.period = 0.0;
        self.voicing = 0.0;
        self.since_detect = 0;
    }

    fn at(&self, pos: f64) -> f32 {
        let base = pos.floor();
        let frac = (pos - base) as f32;
        let i = base as i64;
        let get = |k: i64| self.input[(k.rem_euclid(PSOLA_RING as i64)) as usize];
        get(i) + (get(i + 1) - get(i)) * frac
    }

    /// «Толчок» связок рядом с `pos` (в пределах ±`reach`): середина самого громкого места
    /// по энергии за 1 мс — отдельный отсчёт ненадёжен (затухающие колебания формант).
    fn pulse_near(&self, pos: f64, reach: f64) -> f64 {
        const W: i64 = 48;
        let get = |k: i64| self.input[(k.rem_euclid(PSOLA_RING as i64)) as usize];
        let (from, to) = ((pos - reach).ceil() as i64, (pos + reach).floor() as i64);
        let mut energy: f32 = (from - W / 2..from + W / 2).map(|k| get(k) * get(k)).sum();
        let mut best = (from, energy);
        for j in from + 1..=to {
            let (gone, came) = (get(j - 1 - W / 2), get(j + W / 2 - 1));
            energy += came * came - gone * gone;
            if energy > best.1 {
                best = (j, energy);
            }
        }
        best.0 as f64
    }

    /// Период голоса там, откуда сейчас берутся зёрна (YIN); 0 — шум или тишина. Не по самым
    /// свежим отсчётам: голос успевает сменить высоту, и зёрна легли бы не по периоду.
    fn find_period(&mut self) -> f32 {
        // Ищем на половинной частоте (24 кГц): для голоса хватает, а считать в 4 раза меньше.
        const D: usize = 2;
        let (win, max_lag, min_lag) = (YIN_WIN / D, YIN_MAX_LAG / D, YIN_MIN_LAG / D);
        let n = YIN_WIN + YIN_MAX_LAG;
        let center = self.t as i64 - (PSOLA_LATENCY - PSOLA_LOOKAHEAD) as i64;
        let start = (center - n as i64 / 2).min(self.t as i64 - n as i64);
        let mut energy = 0.0f32;
        let get = |k: i64| self.input[(k.rem_euclid(PSOLA_RING as i64)) as usize];
        for j in 0..n / D {
            // Среднее пары соседних отсчётов — и прореживание, и срез верхов.
            let v = (get(start + (j * D) as i64) + get(start + (j * D) as i64 + 1)) * 0.5;
            self.detect[j] = v;
            energy += v * v;
        }
        if energy / ((n / D) as f32) < 1e-7 {
            return 0.0;
        }
        // Шипящая (с, ш, ф) — шум, а не голос, даже если только что был голос: её не режем по
        // периоду (иначе она «дробится»). Резкость — по полной частоте, по середине окна.
        let (mut e, mut d) = (0.0f32, 0.0f32);
        for k in (n / 4) as i64..(3 * n / 4) as i64 {
            let (a, b) = (get(start + k - 1), get(start + k));
            e += b * b;
            d += (b - a) * (b - a);
        }
        if e > 0.0 && d / e > knob("HISS", HISS_SHARPNESS as f64) as f32 {
            return 0.0;
        }
        // Разность d(τ) = Σ (x[j] − x[j+τ])² и её нормировка (cumulative mean normalized).
        self.diff[0] = 1.0;
        let mut running = 0.0f32;
        for tau in 1..=max_lag {
            let mut d = 0.0f32;
            for j in 0..win {
                let e = self.detect[j] - self.detect[j + tau];
                d += e * e;
            }
            running += d;
            self.diff[tau] = if running > 0.0 { d * tau as f32 / running } else { 1.0 };
        }
        // Первая глубокая яма — период; уточняем по параболе.
        let mut tau = min_lag;
        while tau < max_lag {
            if self.diff[tau] < YIN_THRESHOLD {
                while tau + 1 < max_lag && self.diff[tau + 1] < self.diff[tau] {
                    tau += 1;
                }
                break;
            }
            tau += 1;
        }
        if tau >= max_lag {
            // Глубокой ямы нет — берём самую глубокую (вдруг голос просто быстро меняет высоту).
            tau = (min_lag..max_lag).min_by(|&a, &b| self.diff[a].total_cmp(&self.diff[b])).unwrap_or(min_lag);
        }
        // Голос ли это: яма достаточно глубокая. Если только что был голос — верим и ямам
        // помельче: на быстрой смене высоты период «размыт», но это всё ещё голос (иначе высота
        // на миг возвращается к исходной — «спотыкание»).
        let limit = if self.period > 0.0 { YIN_KEEP_VOICED } else { YIN_VOICED };
        if tau < 1 || tau + 1 > max_lag || self.diff[tau] >= limit {
            return 0.0;
        }
        let (a, b, c) = (self.diff[tau - 1], self.diff[tau], self.diff[tau + 1]);
        let den = a - 2.0 * b + c;
        let shift = if den.abs() > 1e-9 { 0.5 * (a - c) / den } else { 0.0 };
        (tau as f32 + shift.clamp(-1.0, 1.0)) * D as f32
    }

    /// Один отсчёт: вход → выход (с задержкой PSOLA_LATENCY). `pitch` и `timbre` — во сколько
    /// раз сдвинуть высоту и окраску.
    fn tick(&mut self, x: f32, pitch: f32, timbre: f32) -> f32 {
        let now = self.t as usize % PSOLA_RING;
        self.input[now] = x;
        self.t += 1;
        self.since_detect += 1;
        if self.since_detect as f64 >= knob("EVERY", YIN_EVERY as f64) {
            self.since_detect = 0;
            let found = self.find_period();
            // Плавно к новой оценке (не ступенькой): ступеньки высоты — «дрожь» голоса.
            let k = knob("SMOOTH", PERIOD_SMOOTH) as f32;
            self.period = if found > 0.0 && self.period > 0.0 { self.period + (found - self.period) * k } else { found };
        }
        // Зёрна кладём заранее: к моменту, когда до метки останется PSOLA_LOOKAHEAD отсчётов.
        while self.next_mark <= (self.t + PSOLA_LOOKAHEAD as u64) as f64 {
            let voiced = self.period > 0.0;
            let p = if voiced { self.period } else { UNVOICED_STEP } as f64;
            let ratio = if voiced { pitch as f64 } else { 1.0 };
            let target = self.next_mark - PSOLA_LATENCY as f64;
            // Зерно — смесь двух соседних периодов голоса: того, что перед нужным местом, и
            // следующего, по тому, насколько место ближе к каждому. Если брать один период
            // целиком, одни периоды повторяются дважды, другие — нет, и в голосе появляется
            // «зубчатость» с половинной высотой (шершавость, «робот»).
            let blend = if voiced {
                // Метки — на «толчках» связок (самое громкое место рядом): так окно не режет
                // период неровно и окраска голоса не плывёт.
                if (target - self.in_center).abs() > 4.0 * p {
                    self.in_center = self.pulse_near(target, p * 0.5);
                }
                while self.in_center + p <= target {
                    self.in_center = self.pulse_near(self.in_center + p, p * knob("SNAP", SNAP));
                }
                while self.in_center > target {
                    self.in_center = self.pulse_near(self.in_center - p, p * knob("SNAP", SNAP));
                }
                (((target - self.in_center) / p).clamp(0.0, 1.0) * knob("BLEND", 1.0)) as f32
            } else {
                self.in_center = target;
                0.0
            };
            let next_center = self.in_center + p;
            let half = p / timbre as f64;
            let gain = (timbre as f64 / ratio) as f32;
            let first = (self.next_mark - half).ceil() as i64;
            let last = (self.next_mark + half).floor() as i64;
            for o in first..=last {
                let rel = (o as f64 - self.next_mark) / half;
                let w = (0.5 + 0.5 * (std::f64::consts::PI * rel).cos()) as f32;
                let off = (o as f64 - self.next_mark) * timbre as f64;
                let x = if blend > 0.0 {
                    self.at(self.in_center + off) * (1.0 - blend) + self.at(next_center + off) * blend
                } else {
                    self.at(self.in_center + off)
                };
                let v = x * w * gain;
                self.acc[(o.rem_euclid(PSOLA_RING as i64)) as usize] += v;
            }
            self.next_mark += p / ratio;
        }
        // Шум идёт как есть (с той же задержкой), голос — из зёрен; переход плавный (≈5 мс).
        let target = if self.period > 0.0 { 1.0 } else { 0.0 };
        self.voicing += (target - self.voicing).clamp(-VOICING_STEP, VOICING_STEP);
        let grains = self.acc[now];
        self.acc[now] = 0.0;
        let direct = self.input[((self.t as i64 - 1 - PSOLA_LATENCY as i64).rem_euclid(PSOLA_RING as i64)) as usize];
        grains * self.voicing + direct * (1.0 - self.voicing)
    }
}

/// Гребенчатый фильтр с затуханием верхов (Freeverb).
struct Comb {
    buf: Box<[f32]>,
    pos: usize,
    store: f32,
}

impl Comb {
    fn new(len: usize) -> Comb {
        Comb { buf: vec![0.0; len].into_boxed_slice(), pos: 0, store: 0.0 }
    }

    fn run(&mut self, x: f32) -> f32 {
        let y = self.buf[self.pos];
        self.store = y * (1.0 - DAMP) + self.store * DAMP;
        self.buf[self.pos] = x + self.store * ROOM;
        self.pos = (self.pos + 1) % self.buf.len();
        y
    }
}

/// Всепропускающий фильтр (Freeverb): размывает отражения.
struct Allpass {
    buf: Box<[f32]>,
    pos: usize,
}

impl Allpass {
    fn new(len: usize) -> Allpass {
        Allpass { buf: vec![0.0; len].into_boxed_slice(), pos: 0 }
    }

    fn run(&mut self, x: f32) -> f32 {
        let b = self.buf[self.pos];
        self.buf[self.pos] = x + b * 0.5;
        self.pos = (self.pos + 1) % self.buf.len();
        b - x
    }
}

/// Зал: на канал — свои 8 гребенок и 4 всепропускающих фильтра.
struct Reverb {
    combs: [[Comb; 8]; CHANNELS],
    allpasses: [[Allpass; 4]; CHANNELS],
}

impl Reverb {
    fn new() -> Reverb {
        let scaled = |n: usize, ch: usize| (n + ch * STEREO_SPREAD) * RATE as usize / 44_100;
        Reverb {
            combs: std::array::from_fn(|ch| std::array::from_fn(|i| Comb::new(scaled(COMBS[i], ch)))),
            allpasses: std::array::from_fn(|ch| std::array::from_fn(|i| Allpass::new(scaled(ALLPASSES[i], ch)))),
        }
    }

    /// Тишина в зале: буферы обнуляются на месте (без выделения памяти).
    fn clear(&mut self) {
        for c in self.combs.iter_mut().flatten() {
            c.buf.fill(0.0);
            c.store = 0.0;
        }
        for a in self.allpasses.iter_mut().flatten() {
            a.buf.fill(0.0);
        }
    }

    fn run(&mut self, ch: usize, x: f32) -> f32 {
        let input = x * 0.015;
        let mut y = self.combs[ch].iter_mut().map(|c| c.run(input)).sum::<f32>();
        for a in self.allpasses[ch].iter_mut() {
            y = a.run(y);
        }
        y
    }
}

pub struct State {
    voice: Psola,
    growl: Ramp,
    growl_phase: f32,
    growl_low: Biquad,
    chorus_buf: Box<[[f32; CHORUS_BUF]; CHANNELS]>,
    chorus_write: usize,
    chorus_phase: [f32; 3],
    chorus: Ramp,
    reverb: Reverb,
    hall: Ramp,
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
            voice: Psola::new(),
            growl: Ramp::default(),
            growl_phase: 0.0,
            growl_low: Biquad::new(false, GROWL_LOWPASS),
            chorus_buf: Box::new([[0.0; CHORUS_BUF]; CHANNELS]),
            chorus_write: 0,
            chorus_phase: [0.0, 2.1, 4.2],
            chorus: Ramp::default(),
            reverb: Reverb::new(),
            hall: Ramp::default(),
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
        let formant = p.formant.load().clamp(-8.0, 8.0);
        let tone = p.tone.load().clamp(-12.0, 12.0);
        let voice_on = if formant.abs() > 0.01 || tone.abs() > 0.01 { 1.0 } else { 0.0 };
        let timbre_ratio = 2f32.powf(formant / 12.0);
        let tone_ratio = 2f32.powf(tone / 12.0);
        let growl = p.growl.load().clamp(0.0, 1.0);
        let chorus = p.chorus.load().clamp(0.0, 1.0);
        let hall = p.hall.load().clamp(0.0, 1.0);
        // Включаем заново — без остатков старого звука.
        if voice_on > 0.0 && self.voice.mix.last == 0.0 {
            self.voice.reset();
        }
        if hall > 0.0 && self.hall.last == 0.0 {
            self.reverb.clear();
        }
        let frames = buf.len() / CHANNELS;
        let growl_step = TAU * GROWL_HZ / RATE as f32;
        let chorus_steps = CHORUS_VOICES.map(|(_, _, hz)| TAU * hz / RATE as f32);
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
            let mut v = [0.0f32; CHANNELS];
            for (ch, out) in v.iter_mut().enumerate() {
                let x = buf[f * CHANNELS + ch];
                // Высота (линия задержки пишется всегда — чтобы включение было без щелчка).
                self.pitch_buf[ch][self.pitch_write] = x;
                let mut y = x;
                if mix > 0.0 {
                    let shifted = self.pitch_tap(ch, d0) * tap_weight(d0) + self.pitch_tap(ch, d1) * tap_weight(d1);
                    y = x * (1.0 - mix) + shifted * mix;
                }
                *out = y;
            }
            // Высота и тембр (моно: оба канала получают один и тот же голос).
            let voice_now = self.voice.mix.at(voice_on, f, frames);
            if voice_now > 0.0 {
                let wet = self.voice.tick((v[0] + v[1]) * 0.5, tone_ratio, timbre_ratio);
                for y in v.iter_mut() {
                    *y = *y * (1.0 - voice_now) + wet * voice_now;
                }
            }
            let growl_now = self.growl.at(growl, f, frames);
            let chorus_now = self.chorus.at(chorus, f, frames);
            let hall_now = self.hall.at(hall, f, frames);
            let rough = 1.0 - 0.35 * growl_now * (0.5 + 0.5 * self.growl_phase.sin());
            for (ch, &y0) in v.iter().enumerate() {
                let mut y = y0;
                // Робот.
                y *= carrier;
                // Рация: фильтры работают всегда (их состояние не прыгает при включении).
                let band = self.low.run(ch, self.high.run(ch, y));
                let driven = (band * RADIO_DRIVE).tanh() / RADIO_DRIVE.tanh();
                y = y * (1.0 - radio_now) + driven * radio_now;
                // Рык: перегруз (с ограничением), «рычание», срез верхов.
                if growl_now > 0.0 {
                    let drive = 1.0 + growl_now * GROWL_DRIVE;
                    let wet = self.growl_low.run(ch, (y * drive).tanh() / drive.tanh() * rough);
                    y = y * (1.0 - growl_now) + wet * growl_now;
                }
                // Хор: линия задержки пишется всегда.
                self.chorus_buf[ch][self.chorus_write] = y;
                if chorus_now > 0.0 {
                    let mut wet = 0.0;
                    for (i, &(base, depth, _)) in CHORUS_VOICES.iter().enumerate() {
                        let delay = (base + depth * self.chorus_phase[i].sin()) * RATE as f32;
                        wet += self.chorus_tap(ch, delay);
                    }
                    wet /= CHORUS_VOICES.len() as f32;
                    y = y * (1.0 - 0.3 * chorus_now) + wet * 0.9 * chorus_now;
                }
                // Эхо.
                let read = (self.echo_write + ECHO_BUF - ECHO_DELAY) % ECHO_BUF;
                let delayed = self.echo_buf[ch][read];
                self.echo_buf[ch][self.echo_write] = y + delayed * ECHO_FEEDBACK;
                y += delayed * ECHO_LEVEL * echo_now;
                // Зал.
                if hall_now > 0.0 {
                    y += self.reverb.run(ch, y) * 3.0 * HALL_WET * hall_now;
                }
                buf[f * CHANNELS + ch] = if y.is_finite() { y } else { 0.0 };
            }
            self.chorus_write = (self.chorus_write + 1) % CHORUS_BUF;
            self.growl_phase = (self.growl_phase + growl_step) % TAU;
            for (ph, step) in self.chorus_phase.iter_mut().zip(chorus_steps) {
                *ph = (*ph + step) % TAU;
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
        self.voice.mix.last = voice_on;
        self.growl.last = growl;
        self.chorus.last = chorus;
        self.hall.last = hall;
    }

    fn chorus_tap(&self, ch: usize, delay: f32) -> f32 {
        let pos = self.chorus_write as f32 - delay;
        let base = pos.floor();
        let frac = pos - base;
        let i = base as isize;
        let buf = &self.chorus_buf[ch];
        let at = |k: isize| buf[(k.rem_euclid(CHORUS_BUF as isize)) as usize];
        at(i) + (at(i + 1) - at(i)) * frac
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
            ..Default::default()
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

    /// Голос: смесь тонов и шума (как настоящий голос — богаче чистого тона), 1,5 с.
    fn test_voice() -> Vec<f32> {
        let mut seed = 12345u32;
        let n = RATE as usize * 3 / 2;
        (0..n)
            .flat_map(|i| {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                let noise = (seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5;
                let t = i as f32 / RATE as f32;
                let v = 0.3 * (TAU * 180.0 * t).sin() + 0.15 * (TAU * 720.0 * t).sin() + 0.05 * noise;
                [v, v * 0.9]
            })
            .collect()
    }

    /// Готовые голоса звучат ровно как до прокачки морфера (копия того морфера —
    /// `morpher_legacy.rs`). «Пришельца» автор просил сохранить в первозданном виде.
    #[test]
    fn built_in_voices_sound_exactly_as_before() {
        use crate::morpher_legacy as old;
        // (pitch, robot, radio, echo, wobble) — как в app/ui/src/morpher.ts.
        let voices: [(&str, [f32; 5]); 7] = [
            ("normal", [0.0, 0.0, 0.0, 0.0, 0.0]),
            ("monster", [-7.0, 0.0, 0.0, 0.0, 0.0]),
            ("chipmunk", [7.0, 0.0, 0.0, 0.0, 0.0]),
            ("robot", [0.0, 1.0, 0.0, 0.2, 0.0]),
            ("radio", [0.0, 0.0, 1.0, 0.0, 0.0]),
            ("cave", [0.0, 0.0, 0.0, 0.8, 0.0]),
            ("alien", [3.0, 0.0, 0.0, 0.25, 0.7]),
        ];
        let input = test_voice();
        for (name, [pitch, robot, radio, echo, wobble]) in voices {
            let p = params(pitch, robot, radio, echo);
            p.wobble.store(wobble);
            let new = run(&p, &input);
            let lp = old::LegacyParams::default();
            lp.pitch.store(pitch);
            lp.robot.store(robot);
            lp.radio.store(radio);
            lp.echo.store(echo);
            lp.wobble.store(wobble);
            let mut st = old::State::default();
            let mut was = input.clone();
            for chunk in was.chunks_mut(BLOCK_SAMPLES) {
                st.process(chunk, &lp);
            }
            let same = new.iter().zip(&was).all(|(a, b)| a.to_bits() == b.to_bits());
            assert!(same, "{name} звучит иначе, чем до прокачки");
        }
    }

    /// «Голос»: пила 150 Гц (богатая гармониками, как голосовые связки), 1 с.
    fn saw(freq: f32, seconds: f32) -> Vec<f32> {
        let n = (RATE as f32 * seconds) as usize;
        (0..n)
            .flat_map(|i| {
                let ph = (freq * i as f32 / RATE as f32).fract();
                let v = 0.3 * (2.0 * ph - 1.0);
                [v, v]
            })
            .collect()
    }

    /// Центр тяжести спектра (Гц) левого канала до 6 кГц — по мощности, с окном Ханна (без окна
    /// результат зависит от того, где в куске начинается период, — и «плывёт» от задержки).
    fn centroid(x: &[f32]) -> f32 {
        let left: Vec<f32> = x.iter().step_by(2).skip(RATE as usize / 2).take(4096).copied().collect();
        let (mut num, mut den) = (0.0f32, 0.0f32);
        for k in 1..512 {
            let f = k as f32 * RATE as f32 / 4096.0;
            let (mut re, mut im) = (0.0f32, 0.0f32);
            for (n, &v) in left.iter().enumerate() {
                let w = 0.5 - 0.5 * (TAU * n as f32 / 4096.0).cos();
                let a = TAU * k as f32 * n as f32 / 4096.0;
                re += v * w * a.cos();
                im += v * w * a.sin();
            }
            let power = re * re + im * im;
            num += f * power;
            den += power;
        }
        num / den
    }

    /// Первая форманта (Гц): вершина сглаженного (±150 Гц) спектра мощности в 450–3000 Гц —
    /// окраска голоса, не зависящая от высоты (гармоники сглаживание стирает).
    fn first_formant(x: &[f32]) -> f32 {
        let left: Vec<f32> = x.iter().step_by(2).skip(RATE as usize / 2).take(8192).copied().collect();
        let n = left.len();
        let bin_hz = RATE as f32 / n as f32;
        let (lo, hi) = ((450.0 / bin_hz) as usize, (3000.0 / bin_hz) as usize);
        let spread = (150.0 / bin_hz) as usize;
        let power: Vec<f32> = (lo - spread..hi + spread)
            .map(|k| {
                let (mut re, mut im) = (0.0f32, 0.0f32);
                for (j, &v) in left.iter().enumerate() {
                    let w = 0.5 - 0.5 * (TAU * j as f32 / n as f32).cos();
                    let a = TAU * k as f32 * j as f32 / n as f32;
                    re += v * w * a.cos();
                    im += v * w * a.sin();
                }
                re * re + im * im
            })
            .collect();
        let smooth = |i: usize| power[i - spread..=i + spread].iter().sum::<f32>();
        let best = (spread..power.len() - spread).max_by(|&a, &b| smooth(a).partial_cmp(&smooth(b)).unwrap()).unwrap();
        (lo - spread + best) as f32 * bin_hz
    }

    fn with(f: impl Fn(&MorpherParams)) -> MorpherParams {
        let p = MorpherParams::default();
        f(&p);
        p
    }

    /// Гласная «а»: пила 150 Гц через два резонатора (форманты 800 и 1300 Гц), как в речи.
    fn vowel(seconds: f32) -> Vec<f32> {
        let src = saw(150.0, seconds);
        let res = |f: f32| {
            let r = 0.96f32;
            let c = 2.0 * r * (TAU * f / RATE as f32).cos();
            (c, -r * r)
        };
        let filters = [res(800.0), res(1300.0)];
        let mut z = [[0.0f32; 2]; 2];
        src.chunks(2)
            .flat_map(|fr| {
                let x = fr[0];
                let mut y = 0.0;
                for (k, &(c1, c2)) in filters.iter().enumerate() {
                    let v = x * 0.05 + c1 * z[k][0] + c2 * z[k][1];
                    z[k][1] = z[k][0];
                    z[k][0] = v;
                    y += v;
                }
                [y, y]
            })
            .collect()
    }

    /// Обычная высота по кускам — середина ряда (без разгона в начале): редкий промах оценки
    /// (удвоение периода) её не сдвигает.
    fn mean_pitch(x: &[f32]) -> f32 {
        let mut t = pitch_track(x)[8..].to_vec();
        t.sort_by(|a, b| a.partial_cmp(b).unwrap());
        t[t.len() / 2]
    }

    #[test]
    fn timbre_moves_the_colour_but_keeps_the_pitch() {
        let (input, _) = live_voice(1.5);
        let base = first_formant(&input);
        let up = run(&with(|p| p.formant.store(6.0)), &input);
        let down = run(&with(|p| p.formant.store(-6.0)), &input);
        let (cu, cd) = (first_formant(&up), first_formant(&down));
        // ±6 полутонов — в √2 раза; допускаем разброс оценки.
        assert!(cu > base * 1.2 && cd < base * 0.83, "форманта: вниз {cd}, было {base}, вверх {cu}");
        for (name, out) in [("вверх", &up), ("вниз", &down)] {
            // На ровном голосе высота при тембре на месте точно (×1,000); на живом, с быстрой
            // интонацией, при крайнем тембре ±6 оценка высоты расплывается — запас 10 %.
            let ratio = mean_pitch(out) / mean_pitch(&input);
            assert!((ratio - 1.0).abs() < 0.1, "{name}: высота ×{ratio}");
            let level = rms(&out[RATE as usize..]) / rms(&input[RATE as usize..]);
            assert!((0.4..2.5).contains(&level), "{name}: уровень ×{level}");
        }
    }

    #[test]
    fn natural_pitch_keeps_the_colour() {
        let (input, _) = live_voice(1.5);
        let base = first_formant(&input);
        for semi in [7.0f32, -5.0] {
            let out = run(&with(|p| p.tone.store(semi)), &input);
            let ratio = mean_pitch(&out) / mean_pitch(&input);
            let expect = 2f32.powf(semi / 12.0);
            assert!((ratio / expect - 1.0).abs() < 0.04, "{semi}: высота ×{ratio}, ждали ×{expect}");
            // Окраска на месте: первая форманта почти та же (у мультяшной высоты она уехала бы
            // на те же полутоны).
            let c = first_formant(&out);
            // Запас — одна гармоника (у высокого голоса они реже); мультяшная сдвинула бы в 1,5 раза.
            assert!((c / base - 1.0).abs() < 0.2, "{semi}: форманта {c}, было {base}");
            let level = rms(&out[RATE as usize..]) / rms(&input[RATE as usize..]);
            assert!((0.4..2.5).contains(&level), "{semi}: уровень ×{level}");
        }
    }

    #[test]
    fn hiss_is_not_pitch_shifted() {
        // Шум (как «с» или «ш»): высоту ему не меняем — спектр выше 5 кГц остаётся на месте.
        let mut seed = 7u32;
        let input: Vec<f32> = (0..RATE as usize)
            .flat_map(|_| {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                let v = ((seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 0.3;
                [v, v]
            })
            .collect();
        let out = run(&with(|p| p.tone.store(7.0)), &input);
        let level = rms(&out[RATE as usize / 2..]) / rms(&input[RATE as usize / 2..]);
        assert!((0.5..2.0).contains(&level), "уровень шума ×{level}");
        let c_in = centroid(&input);
        let c_out = centroid(&out);
        assert!((c_out / c_in - 1.0).abs() < 0.2, "центр шума {c_out}, было {c_in}");
    }

    #[test]
    fn growl_adds_rasp_without_getting_much_louder() {
        let input = sine(200.0, 0.5);
        let out = run(&with(|p| p.growl.store(1.0)), &input);
        let hp = |x: &[f32]| rms(&x.iter().step_by(2).collect::<Vec<_>>().windows(2).map(|w| w[1] - w[0]).collect::<Vec<_>>());
        assert!(hp(&out) > hp(&input) * 2.0, "верхов: {} против {}", hp(&out), hp(&input));
        let peak = out.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak < 1.05, "пик {peak}");
    }

    #[test]
    fn chorus_thickens_the_voice() {
        let input = sine(300.0, 0.5);
        let out = run(&with(|p| p.chorus.store(1.0)), &input);
        let diff: Vec<f32> = out.iter().zip(&input).map(|(a, b)| a - b).collect();
        assert!(rms(&diff[RATE as usize / 2..]) > 0.05, "хор не слышен");
        let ratio = rms(&out[RATE as usize / 2..]) / rms(&input[RATE as usize / 2..]);
        assert!((0.5..1.6).contains(&ratio), "уровень ×{ratio}");
    }

    #[test]
    fn hall_leaves_a_tail() {
        let mut input = vec![0.0f32; RATE as usize * CHANNELS];
        for v in input[..RATE as usize / 50 * CHANNELS].iter_mut() {
            *v = 0.5;
        }
        let out = run(&with(|p| p.hall.store(1.0)), &input);
        // Через полсекунды после короткого звука зал ещё звучит; сухой голос — молчал бы.
        let tail = rms(&out[RATE as usize / 2 * CHANNELS..RATE as usize * 3 / 4 * CHANNELS]);
        assert!(tail > 0.002, "хвост {tail}");
        assert!(out.iter().all(|v| v.abs() < 1.5));
    }

    #[test]
    fn new_effects_switch_without_jumps() {
        let input = sine(200.0, 1.5);
        let mut st = State::default();
        let p = MorpherParams::default();
        let mut out = input.clone();
        for (i, chunk) in out.chunks_mut(BLOCK_SAMPLES).enumerate() {
            let v = [(0.0, 0.0, 0.0, 0.0), (4.0, 0.0, 0.0, 0.0), (0.0, 1.0, 0.0, 0.0), (0.0, 0.0, 1.0, 0.0), (0.0, 0.0, 0.0, 1.0), (-4.0, 0.6, 0.6, 0.6)]
                [(i / 12) % 6];
            p.formant.store(v.0);
            p.tone.store(-v.0);
            p.growl.store(v.1);
            p.chorus.store(v.2);
            p.hall.store(v.3);
            st.process(chunk, &p);
        }
        // Тембр с плавным переходом, остальные — тоже: скачков между соседними отсчётами нет.
        let left: Vec<f32> = out.iter().step_by(2).copied().collect();
        let step = left.windows(2).fold(0.0f32, |m, w| m.max((w[1] - w[0]).abs()));
        assert!(step < 0.2, "скачок {step}");
        assert!(out.iter().all(|v| v.is_finite()));
    }

    /// Скорость: 10 с голоса со всеми эффектами сразу. Запуск вручную, в release:
    /// `cargo test --release -p engine morpher_speed -- --ignored --nocapture`.
    #[test]
    #[ignore = "замер скорости"]
    fn morpher_speed() {
        let input = vowel(10.0);
        let p = with(|p| {
            p.pitch.store(-6.0);
            p.wobble.store(0.3);
            p.robot.store(0.3);
            p.radio.store(0.3);
            p.echo.store(0.3);
            p.formant.store(-2.0);
            p.tone.store(3.0);
            p.growl.store(0.8);
            p.chorus.store(0.5);
            p.hall.store(0.3);
        });
        let start = std::time::Instant::now();
        let _ = run(&p, &input);
        let spent = start.elapsed().as_secs_f64();
        println!("10 с звука за {:.1} мс — {:.2} % одного ядра", spent * 1000.0, spent / 10.0 * 100.0);
        assert!(spent < 1.0);
    }

    /// «Живой» голос: импульсы голосовых связок с плавающей высотой (130–160 Гц, 4 раза в
    /// секунду, как интонация) через три форманты (700, 1200, 2600 Гц) и немного дыхания.
    fn live_voice(seconds: f32) -> (Vec<f32>, Vec<f32>) {
        let n = (RATE as f32 * seconds) as usize;
        let mut seed = 99u32;
        let mut phase = 0.0f32;
        let mut f0s = Vec::with_capacity(n);
        let res = |f: f32, bw: f32| {
            let r = (-PI * bw / RATE as f32).exp();
            (2.0 * r * (TAU * f / RATE as f32).cos(), -r * r)
        };
        let filters = [res(700.0, 110.0), res(1200.0, 120.0), res(2600.0, 160.0)];
        let gains = [1.0f32, 0.6, 0.25];
        let mut z = [[0.0f32; 2]; 3];
        let mut out = Vec::with_capacity(n * 2);
        for i in 0..n {
            let t = i as f32 / RATE as f32;
            let f0 = 145.0 + 15.0 * (TAU * 4.0 * t).sin();
            f0s.push(f0);
            phase += f0 / RATE as f32;
            let pulse = if phase >= 1.0 {
                phase -= 1.0;
                1.0
            } else {
                0.0
            };
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let breath = ((seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 0.02;
            let x = pulse + breath;
            let mut y = 0.0;
            for k in 0..3 {
                let (c1, c2) = filters[k];
                let v = x + c1 * z[k][0] + c2 * z[k][1];
                z[k][1] = z[k][0];
                z[k][0] = v;
                y += v * gains[k] * 0.02;
            }
            out.push(y);
            out.push(y);
        }
        (out, f0s)
    }

    /// Высота по кускам 40 мс — тем же способом, что и в морфере (YIN, 70–600 Гц).
    fn pitch_track(x: &[f32]) -> Vec<f32> {
        let left: Vec<f32> = x.iter().step_by(2).copied().collect();
        let win = RATE as usize / 25;
        let (lo, hi) = (RATE as usize / 600, RATE as usize / 70);
        let w = win - hi;
        left.chunks(win)
            .filter(|c| c.len() == win)
            .map(|c| {
                let mut cmnd = vec![1.0f32; hi + 1];
                let mut running = 0.0f32;
                for tau in 1..=hi {
                    let d: f32 = (0..w).map(|j| (c[j] - c[j + tau]).powi(2)).sum();
                    running += d;
                    cmnd[tau] = if running > 0.0 { d * tau as f32 / running } else { 1.0 };
                }
                let mut tau = lo;
                while tau < hi && cmnd[tau] >= 0.15 {
                    tau += 1;
                }
                while tau + 1 < hi && cmnd[tau + 1] < cmnd[tau] {
                    tau += 1;
                }
                if tau >= hi {
                    tau = (lo..hi).min_by(|&a, &b| cmnd[a].partial_cmp(&cmnd[b]).unwrap()).unwrap();
                }
                RATE as f32 / tau as f32
            })
            .collect()
    }

    /// На живом голосе (высота плавает) естественная высота идёт за интонацией: каждый кусок
    /// выше ровно во столько раз, во сколько просили. «Робот» — когда высота перестаёт плавать
    /// или застревает на частоте кадров движка.
    #[test]
    fn natural_pitch_follows_a_live_voice() {
        let (input, f0s) = live_voice(2.0);
        let win = RATE as usize / 25;
        for semi in [5.0f32, -4.0] {
            let ratio = 2f32.powf(semi / 12.0);
            let out = run(&with(|p| p.tone.store(semi)), &input);
            let track = pitch_track(&out);
            let mut bad = 0;
            let mut total = 0;
            // Первые 0,3 с — разгон; середину куска сравниваем с голосом на задержку движка раньше.
            for (j, &got) in track.iter().enumerate().skip(8) {
                let src = (j * win + win / 2).saturating_sub(PSOLA_LATENCY);
                let expect = f0s[src.min(f0s.len() - 1)] * ratio;
                total += 1;
                if (got - expect).abs() / expect > 0.06 {
                    bad += 1;
                }
            }
            // Терпим единичные промахи самой оценки высоты (не больше 10 % кусков).
            assert!(bad * 10 <= total, "{semi}: мимо высоты {bad} из {total} кусков: {track:?}");
        }
    }

    /// WAV (моно, float32 или int16) → отсчёты, только для ручных разборов.
    fn read_wav(path: &str) -> Vec<f32> {
        let b = std::fs::read(path).expect("wav");
        let mut i = 12;
        let (mut fmt, mut bits) = (1u16, 16u16);
        while i + 8 <= b.len() {
            let id = &b[i..i + 4];
            let len = u32::from_le_bytes(b[i + 4..i + 8].try_into().unwrap()) as usize;
            if id == b"fmt " {
                fmt = u16::from_le_bytes([b[i + 8], b[i + 9]]);
                bits = u16::from_le_bytes([b[i + 22], b[i + 23]]);
            }
            if id == b"data" {
                let d = &b[i + 8..(i + 8 + len).min(b.len())];
                return if fmt == 3 && bits == 32 {
                    d.chunks(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect()
                } else {
                    d.chunks(2).map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0).collect()
                };
            }
            i += 8 + len;
        }
        panic!("нет данных");
    }

    /// Ручной разбор записи: SB_WAV=путь к WAV (48 кГц, моно). Печатает высоту по кускам 40 мс.
    #[test]
    #[ignore = "ручной разбор записи"]
    fn analyze_recording() {
        let mono = read_wav(&std::env::var("SB_WAV").expect("SB_WAV"));
        let stereo: Vec<f32> = mono.iter().flat_map(|&v| [v, v]).collect();
        let track = pitch_track(&stereo);
        let win = RATE as usize / 25;
        let levels: Vec<f32> = mono.chunks(win).map(|c| (c.iter().map(|v| v * v).sum::<f32>() / c.len() as f32).sqrt()).collect();
        let line: Vec<String> = track.iter().zip(&levels).map(|(f, l)| if *l > 0.005 { format!("{f:.0}") } else { "·".into() }).collect();
        println!("высота по 40 мс: {}", line.join(" "));
    }

    /// Ручное прослушивание: SB_WAV — запись голоса без морфера (WAV 48 кГц, моно), SB_OUT —
    /// папка. Пишет туда тот же голос через несколько настроек — сравнить на слух.
    #[test]
    #[ignore = "ручное прослушивание"]
    fn render_recording() {
        let mono = read_wav(&std::env::var("SB_WAV").expect("SB_WAV"));
        let dir = std::env::var("SB_OUT").expect("SB_OUT");
        let stereo: Vec<f32> = mono.iter().flat_map(|&v| [v, v]).collect();
        let variants: [(&str, f32, f32); 5] = [("girl", 5.0, 2.0), ("up3", 3.0, 0.0), ("down4", -4.0, 0.0), ("timbre_up3", 0.0, 3.0), ("timbre_down3", 0.0, -3.0)];
        for (name, tone, formant) in variants {
            let out = run(&with(|p| { p.tone.store(tone); p.formant.store(formant); }), &stereo);
            let left: Vec<f32> = out.iter().step_by(2).copied().collect();
            let mut b = Vec::new();
            let data = (left.len() * 4) as u32;
            b.extend_from_slice(b"RIFF");
            b.extend_from_slice(&(36 + data).to_le_bytes());
            b.extend_from_slice(b"WAVEfmt ");
            for v in [16u32.to_le_bytes(), [3, 0, 1, 0], RATE.to_le_bytes(), (RATE * 4).to_le_bytes(), [4, 0, 32, 0]] {
                b.extend_from_slice(&v);
            }
            b.extend_from_slice(b"data");
            b.extend_from_slice(&data.to_le_bytes());
            for v in left {
                b.extend_from_slice(&v.to_le_bytes());
            }
            std::fs::write(format!("{dir}/{name}.wav"), b).unwrap();
        }
    }

    /// «Периодичность» голоса по кускам 40 мс: высота нормированной автокорреляции на периоде
    /// (1 — идеально ровный «гудок», живой голос — 0,7–0,9). Средняя по громким кускам.
    fn periodicity(mono: &[f32]) -> f32 {
        let win = RATE as usize / 25;
        let (lo, hi) = (RATE as usize / 400, RATE as usize / 70);
        let mut vals = Vec::new();
        for c in mono.chunks(win).filter(|c| c.len() == win) {
            let level = (c.iter().map(|v| v * v).sum::<f32>() / win as f32).sqrt();
            if level < 0.01 {
                continue;
            }
            let best = (lo..hi)
                .map(|lag| {
                    let (a, b) = (&c[..win - lag], &c[lag..]);
                    let xy: f32 = a.iter().zip(b).map(|(p, q)| p * q).sum();
                    let e = (a.iter().map(|v| v * v).sum::<f32>() * b.iter().map(|v| v * v).sum::<f32>()).sqrt();
                    if e > 0.0 { xy / e } else { 0.0 }
                })
                .fold(0.0f32, f32::max);
            vals.push(best);
        }
        vals.iter().sum::<f32>() / vals.len().max(1) as f32
    }

    /// Ручной разбор: SB_WAV — исходный голос, SB_OUT — папка с обработанными (render_recording).
    #[test]
    #[ignore = "ручной разбор записи"]
    fn compare_periodicity() {
        let dir = std::env::var("SB_OUT").expect("SB_OUT");
        let src = read_wav(&std::env::var("SB_WAV").expect("SB_WAV"));
        println!("исходный: {:.3}", periodicity(&src));
        for name in ["girl", "up3", "down4", "timbre_up3", "timbre_down3"] {
            let x = read_wav(&format!("{dir}/{name}.wav"));
            println!("{name}: {:.3}", periodicity(&x));
        }
    }

    /// Шипящие куски (10 мс): «резкость» (энергия разности соседних отсчётов к энергии) выше
    /// 0,2 — у «с» (4–10 кГц) около 0,5–1,5, у гласных — сотые.
    fn hiss_chunks(mono: &[f32]) -> Vec<usize> {
        let win = RATE as usize / 100;
        mono.chunks(win)
            .enumerate()
            .filter(|(_, c)| {
                let e: f32 = c.iter().map(|v| v * v).sum();
                let d: f32 = c.windows(2).map(|w| (w[1] - w[0]).powi(2)).sum();
                e / c.len() as f32 > 1e-6 && d / e > 0.2
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// «Дробление» шума: насколько сильно скачет громкость верхов по кускам 2 мс внутри шипящих
    /// (разброс к среднему). У исходной шипящей — ровно, у раздробленной — «пульсирует».
    fn hiss_flutter(mono: &[f32], chunks: &[usize], delay: usize) -> f32 {
        let win = RATE as usize / 100;
        let sub = RATE as usize / 500;
        let mut vals = Vec::new();
        for &c in chunks {
            let from = c * win + delay;
            if from + win + 1 > mono.len() {
                continue;
            }
            let hp: Vec<f32> = mono[from..from + win + 1].windows(2).map(|w| w[1] - w[0]).collect();
            let levels: Vec<f32> = hp.chunks(sub).map(|s| (s.iter().map(|v| v * v).sum::<f32>() / s.len() as f32).sqrt()).collect();
            let mean = levels.iter().sum::<f32>() / levels.len() as f32;
            let var = levels.iter().map(|l| (l - mean).powi(2)).sum::<f32>() / levels.len() as f32;
            if mean > 0.0 {
                vals.push(var.sqrt() / mean);
            }
        }
        vals.iter().sum::<f32>() / vals.len().max(1) as f32
    }

    /// Ручной разбор шипящих: SB_WAV — исходный голос. Сколько шипящих морфер принял за голос
    /// и насколько он их «дробит».
    #[test]
    #[ignore = "ручной разбор записи"]
    fn analyze_hiss() {
        let mono = read_wav(&std::env::var("SB_WAV").expect("SB_WAV"));
        let chunks = hiss_chunks(&mono);
        // Как движок видит шипящие: голос или шум.
        let mut ps = Psola::new();
        ps.reset();
        let win = RATE as usize / 100;
        let mut voiced_in_hiss = 0;
        let mut periods = vec![0.0f32; mono.len() / win + 1];
        for (i, &x) in mono.iter().enumerate() {
            ps.tick(x, 1.33, 1.12);
            // Решение относится к месту, откуда берутся зёрна, — на (задержка − запас) раньше.
            let at = i.saturating_sub(PSOLA_LATENCY - PSOLA_LOOKAHEAD) / win;
            periods[at] = ps.period;
        }
        for &c in &chunks {
            if periods[c] > 0.0 {
                voiced_in_hiss += 1;
            }
        }
        let stereo: Vec<f32> = mono.iter().flat_map(|&v| [v, v]).collect();
        let out: Vec<f32> = run(&with(|p| { p.tone.store(5.0); p.formant.store(2.0); }), &stereo).iter().step_by(2).copied().collect();
        println!("шипящих кусков: {}, из них морфер считал голосом: {}", chunks.len(), voiced_in_hiss);
        println!("дробление шипящих: исходный {:.3}, «Девушка» {:.3}", hiss_flutter(&mono, &chunks, 0), hiss_flutter(&out, &chunks, PSOLA_LATENCY));
    }

    /// Шипящая не дробится: шум (как «с») с высотой и тембром — громкость верхов по кускам 2 мс
    /// скачет не больше, чем у исходного (раньше шум тоже резался на зёрна и «пульсировал»).
    #[test]
    fn hiss_does_not_flutter() {
        let mut seed = 3u32;
        let mut z = 0.0f32;
        let mono: Vec<f32> = (0..RATE as usize)
            .map(|_| {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                let n = (seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5;
                // Верха, как у «с»: разность шума.
                let v = n - z;
                z = n;
                v * 0.1
            })
            .collect();
        let stereo: Vec<f32> = mono.iter().flat_map(|&v| [v, v]).collect();
        let out: Vec<f32> = run(&with(|p| { p.tone.store(5.0); p.formant.store(2.0); }), &stereo).iter().step_by(2).copied().collect();
        let chunks: Vec<usize> = (20..90).collect();
        let (a, b) = (hiss_flutter(&mono, &chunks, 0), hiss_flutter(&out, &chunks, PSOLA_LATENCY));
        assert!(b < a * 1.1, "дробление: было {a}, стало {b}");
    }

    /// Ручное прослушивание готовых голосов: SB_WAV — голос без морфера, SB_OUT — папка.
    /// (pitch, robot, radio, echo, wobble, formant, growl, chorus, hall, tone) — как в morpher.ts.
    #[test]
    #[ignore = "ручное прослушивание"]
    fn render_voices() {
        let mono = read_wav(&std::env::var("SB_WAV").expect("SB_WAV"));
        let dir = std::env::var("SB_OUT").expect("SB_OUT");
        let stereo: Vec<f32> = mono.iter().flat_map(|&v| [v, v]).collect();
        let voices: [(&str, [f32; 10]); 9] = [
            ("giant", [-5.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.2, 0.0]),
            ("legion_old", [-2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0]),
            ("legion_new", [0.0, 0.0, 0.0, 0.0, 0.0, -2.0, 0.0, 1.0, 0.25, -3.0]),
            ("demon_old", [-6.0, 0.0, 0.0, 0.0, 0.0, -2.0, 0.8, 0.5, 0.3, 0.0]),
            ("demon_new", [0.0, 0.0, 0.0, 0.0, 0.0, -3.0, 0.4, 0.25, 0.3, -7.0]),
            ("cathedral_old", [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0]),
            ("cathedral_new", [0.0, 0.0, 0.0, 0.15, 0.0, -1.0, 0.0, 0.0, 1.0, -1.0]),
            ("ghost_old", [2.0, 0.0, 0.0, 0.0, 0.2, 0.0, 0.0, 0.4, 0.7, 0.0]),
            ("ghost_new", [0.0, 0.0, 0.15, 0.0, 0.0, 3.0, 0.0, 0.5, 0.8, 3.0]),
        ];
        for (name, v) in voices {
            let p = with(|p| {
                for (cell, x) in [&p.pitch, &p.robot, &p.radio, &p.echo, &p.wobble, &p.formant, &p.growl, &p.chorus, &p.hall, &p.tone].into_iter().zip(v) {
                    cell.store(x);
                }
            });
            let out = run(&p, &stereo);
            let left: Vec<f32> = out.iter().step_by(2).copied().collect();
            let mut b = Vec::new();
            let data = (left.len() * 4) as u32;
            b.extend_from_slice(b"RIFF");
            b.extend_from_slice(&(36 + data).to_le_bytes());
            b.extend_from_slice(b"WAVEfmt ");
            for v in [16u32.to_le_bytes(), [3, 0, 1, 0], RATE.to_le_bytes(), (RATE * 4).to_le_bytes(), [4, 0, 32, 0]] {
                b.extend_from_slice(&v);
            }
            b.extend_from_slice(b"data");
            b.extend_from_slice(&data.to_le_bytes());
            for v in left {
                b.extend_from_slice(&v.to_le_bytes());
            }
            std::fs::write(format!("{dir}/{name}.wav"), b).unwrap();
        }
    }
}
