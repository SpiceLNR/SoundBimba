// SPDX-License-Identifier: AGPL-3.0-or-later
// Звук заставки темы «Night Hub»: свой короткий бас-рифф (≈3 с) и щелчок мыши, синтезируются на
// месте через Web Audio — ни чужих записей, ни файлов. Играет через устройство Windows по
// умолчанию, как любой звук окна, мимо звукового движка: в виртуальный микрофон и схему он не
// попадает. Ровный ритм — удар каждые 0,36 с, без двойных ударов (с ними рифф «ускорялся»):
// A1 с подъездом сверху, C2, D2, E2 с подъездом, щелчок мыши по «▶», долгая A1 в конце (ля
// минор). Время ударов совпадает со сменой значков в theme.css.
//
// Без компрессора (он «качал» громкость — звук хрипел) и с мягкими краями нот (без щелчков на
// стыках). Громкость — по замеру отрисовки риффа (OfflineAudioContext, 2026-11-19).

/** Громкость всего риффа. */
const LEVEL = 0.17;

/** Ноты: начало, с какой частоты подъехать, частота, длина (секунды). */
const NOTES: [number, number, number, number][] = [
  [0.1, 110, 55, 0.3],
  [0.46, 65.41, 65.41, 0.3],
  [0.82, 73.42, 73.42, 0.3],
  [1.18, 130.8, 82.41, 0.3],
  [1.9, 55, 55, 1.4],
];

/** Когда щёлкает мышь — на свою долю сетки, между последним коротким ударом и долгой нотой. */
const CLICK_AT = 1.54;

/** Удар баса: чистый тон и тихая пила октавой выше, мягкий фильтр; края ноты плавные. */
function hit(ctx: BaseAudioContext, out: AudioNode, at: number, from: number, to: number, length: number) {
  const t = at;
  const release = Math.min(0.06, length * 0.4);
  const filter = ctx.createBiquadFilter();
  filter.type = 'lowpass';
  filter.Q.value = 1.2;
  filter.frequency.setValueAtTime(1500, t);
  filter.frequency.exponentialRampToValueAtTime(320, t + Math.min(0.4, length));
  const gain = ctx.createGain();
  gain.gain.setValueAtTime(0, t);
  gain.gain.linearRampToValueAtTime(1, t + 0.012);
  gain.gain.setTargetAtTime(0.6, t + 0.012, 0.05);
  gain.gain.setTargetAtTime(0, t + length - release, release / 3);
  filter.connect(gain).connect(out);
  for (const [type, mult, level] of [['sine', 1, 1], ['sawtooth', 2, 0.18]] as const) {
    const osc = ctx.createOscillator();
    osc.type = type;
    osc.frequency.setValueAtTime(from * mult, t);
    osc.frequency.exponentialRampToValueAtTime(to * mult, t + 0.06);
    const g = ctx.createGain();
    g.gain.value = level;
    osc.connect(g).connect(filter);
    osc.start(t);
    osc.stop(t + length + 0.1);
  }
}

/** Щелчок кнопки мыши: короткий треск (нажатие) и тише — отпускание. */
function click(ctx: BaseAudioContext, out: AudioNode, at: number) {
  const noise = ctx.createBuffer(1, Math.round(ctx.sampleRate * 0.012), ctx.sampleRate);
  const data = noise.getChannelData(0);
  let seed = 7;
  for (let i = 0; i < data.length; i++) {
    seed = (seed * 16807) % 2147483647;
    data[i] = ((seed / 2147483647) * 2 - 1) * (1 - i / data.length) ** 3;
  }
  for (const [offset, level] of [[0, 0.9], [0.075, 0.45]] as const) {
    const src = ctx.createBufferSource();
    src.buffer = noise;
    const band = ctx.createBiquadFilter();
    band.type = 'bandpass';
    band.frequency.value = 3200;
    band.Q.value = 1.2;
    const g = ctx.createGain();
    g.gain.value = level;
    src.connect(band).connect(g).connect(out);
    src.start(at + offset);
  }
}

/** Разложить рифф в контекст, начиная с `start` (секунды контекста). */
export function scheduleRiff(ctx: BaseAudioContext, start: number): void {
  const master = ctx.createGain();
  master.gain.value = LEVEL;
  master.connect(ctx.destination);
  for (const [at, from, to, length] of NOTES) hit(ctx, master, start + at, from, to, length);
  click(ctx, master, start + CLICK_AT);
}

/** Сыграть рифф. Если окно не может играть звук (нет Web Audio, запрет без нажатия) — тихо. */
export function playIntroSound(): void {
  const Ctx = window.AudioContext;
  if (!Ctx) return;
  let ctx: AudioContext;
  try {
    ctx = new Ctx();
  } catch {
    return;
  }
  void ctx.resume().catch(() => {});
  scheduleRiff(ctx, ctx.currentTime + 0.02);
  window.setTimeout(() => void ctx.close().catch(() => {}), 4200);
}
