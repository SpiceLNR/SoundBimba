// SPDX-License-Identifier: AGPL-3.0-or-later
// Готовые голоса морфера — сочетания эффектов движка (crates/engine/src/morpher.rs):
// высота в полутонах (−12…12), «робот», «рация», эхо, дрожание высоты — сила 0…1; с 2026-11-12
// ещё тембр (сдвиг формант, полутона −8…8), рык, хор и зал — сила 0…1.
import type { Params } from './graph';

export interface MorpherVoice {
  id: string;
  params: Effects;
}

/** Все эффекты морфера. Новые добавлять в конец; у старых голосов новые эффекты — 0. */
export const KEYS = ['pitch', 'robot', 'radio', 'echo', 'wobble', 'formant', 'growl', 'chorus', 'hall', 'tone', 'lowcut', 'bright'] as const;
type Effects = Record<(typeof KEYS)[number], number>;

/**
 * Подстройка под голос (как Voice Doctor в MorphVOX). Готовые голоса с естественной высотой
 * (`tone`) настроены на голос автора — 123 Гц (замер по его записи 2026-11-13). Для другого
 * голоса они сдвигаются так, чтобы прийти к той же высоте: у низкого голоса «Девушка» поднимает
 * сильнее, у высокого — меньше, но направление не меняется (высокий голос «Девушка» не опускает,
 * а оставляет как есть). Тембр следует за добавкой высоты на 40 % (как советуют для смены пола).
 */
export const REFERENCE_F0 = 123;
const TIMBRE_FOLLOWS = 0.4;
let userF0: number | null = null;

/** Чей голос подстраиваем: высота, Гц, или null — средний (как у автора). */
export function setVoiceProfile(f0: number | null | undefined): void {
  userF0 = typeof f0 === 'number' && f0 > 50 && f0 < 600 ? f0 : null;
}

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

/** Готовый голос под голос человека (при подстройке). */
export function adapted(p: Effects, f0: number | null = userF0): Effects {
  if (f0 === null || p.tone === 0) return p;
  const extra = 12 * Math.log2(REFERENCE_F0 / f0);
  let tone = p.tone + extra;
  // Направление голоса не меняем: «вверх» не опускает, «вниз» не поднимает.
  tone = p.tone > 0 ? Math.max(0, tone) : Math.min(0, tone);
  tone = Math.round(clamp(tone, -12, 12));
  const formant = Math.round(clamp(p.formant + (tone - p.tone) * TIMBRE_FOLLOWS, -8, 8));
  return { ...p, tone, formant };
}

/** Голос из нескольких эффектов (остальные — 0). */
const voice = (id: string, set: Partial<Effects>): MorpherVoice => ({
  id,
  params: { ...(Object.fromEntries(KEYS.map((k) => [k, 0])) as Effects), ...set },
});

export const VOICES: MorpherVoice[] = [
  voice('normal', {}),
  voice('monster', { pitch: -7 }),
  voice('chipmunk', { pitch: 7 }),
  voice('robot', { robot: 1, echo: 0.2 }),
  voice('radio', { radio: 1 }),
  voice('cave', { echo: 0.8 }),
  // «Плавающий» инопланетный голос: чуть выше, высота непрерывно дрожит, лёгкое эхо корабля.
  // (Первая версия — «чуть выше + робот» — автору не понравилась: смесь двух других голосов.)
  // Не менять: автор просил сохранить «Пришельца» в первозданном виде.
  voice('alien', { pitch: 3, echo: 0.25, wobble: 0.7 }),
  // 2026-11-12, на новых эффектах. «Мультяшная» высота (pitch) двигает и окраску голоса;
  // естественная (tone) — только высоту, окраску задаёт тембр (formant), как в MorphVOX и AV.
  // «Девушка»: выше на 5 полутонов и окраска легче на 2 — без писка бурундука.
  voice('girl', { tone: 5, formant: 2 }),
  voice('giant', { pitch: -5, formant: 2, hall: 0.2 }),
  // 2026-11-13, выбраны автором на слух (его голос, запись 11): основа — естественная высота и
  // тембр, эффекты — понемногу.
  voice('legion', { tone: -3, formant: -2, chorus: 1, hall: 0.25 }),
  voice('demon', { tone: -7, formant: -3, growl: 0.4, chorus: 0.25, hall: 0.3 }),
  voice('cathedral', { tone: -1, formant: -1, echo: 0.15, hall: 1 }),
];


/** Свой голос, сохранённый человеком. Хранится в самом узле морфера (`params.voices`) — поэтому
 *  попадает в проект и в шаблон вместе со схемой. */
export interface CustomVoice {
  id: string;
  name: string;
  params: Effects;
}

/** Голос в списке: готовый (название из перевода; `changed` — перезаписан в этом узле) или свой
 *  (название, которое дал человек). */
export type ShownVoice = { id: string; params: Effects } & ({ custom: false; changed: boolean } | { custom: true; name: string });

/** «Обычный» убрать нельзя: им голос возвращается к исходному. */
export const KEEP_VOICE = 'normal';

const num = (v: unknown) => (typeof v === 'number' && Number.isFinite(v) ? v : 0);
const effectsOf = (params: Params): Effects =>
  Object.fromEntries(KEYS.map((k) => [k, num(params[k])])) as Effects;

/** Свои голоса узла (испорченные записи пропускаются). */
export function customVoices(params: Params): CustomVoice[] {
  const list = Array.isArray(params.voices) ? params.voices : [];
  return list.flatMap((v) => {
    if (typeof v !== 'object' || v === null) return [];
    const { id, name, params: p } = v as Record<string, unknown>;
    if (typeof id !== 'string' || typeof name !== 'string' || typeof p !== 'object' || p === null) return [];
    return [{ id, name, params: effectsOf(p as Params) }];
  });
}

/** Готовые голоса, которые человек убрал из списка. */
export function hiddenVoices(params: Params): string[] {
  return Array.isArray(params.hiddenVoices) ? params.hiddenVoices.filter((x): x is string => typeof x === 'string') : [];
}

/**
 * Перезаписанные готовые голоса этого узла (`params.voiceOverrides`: id → сила эффектов). Сами
 * готовые голоса в программе не меняются — перезапись живёт в узле (значит, и в шаблоне) и
 * снимается «Вернуть как было». «Обычный» не перезаписывается: им голос возвращается к исходному.
 */
export function voiceOverrides(params: Params): Record<string, Effects> {
  const raw = params.voiceOverrides;
  if (typeof raw !== 'object' || raw === null || Array.isArray(raw)) return {};
  const out: Record<string, Effects> = {};
  for (const [id, p] of Object.entries(raw as Record<string, unknown>)) {
    if (id !== KEEP_VOICE && VOICES.some((v) => v.id === id) && typeof p === 'object' && p !== null) out[id] = effectsOf(p as Params);
  }
  return out;
}

/** Голоса в списке узла: готовые (кроме убранных; перезаписанные — с настройкой узла), затем свои. */
export function voicesOf(params: Params): ShownVoice[] {
  const hidden = new Set(hiddenVoices(params));
  const changed = voiceOverrides(params);
  return [
    ...VOICES.filter((v) => v.id === KEEP_VOICE || !hidden.has(v.id)).map((v) => ({
      ...v,
      // Перезаписанный голос — ровно как его настроили (под голос человека уже не подстраивается).
      params: changed[v.id] ?? adapted(v.params),
      custom: false as const,
      changed: v.id in changed,
    })),
    ...customVoices(params).map((v) => ({ ...v, custom: true as const })),
  ];
}

/** Какой голос выбран (все значения совпадают), иначе null — своя настройка. */
export function voiceOf(params: Params): string | null {
  const now = effectsOf(params);
  return voicesOf(params).find((v) => KEYS.every((k) => now[k] === v.params[k]))?.id ?? null;
}

/** Сохранить текущую настройку как свой голос (он же становится последним выбранным). */
export function addVoice(params: Params, name: string): Params {
  const list = customVoices(params);
  let n = list.length + 1;
  while (list.some((v) => v.id === `c${n}`)) n++;
  return { voices: [...list, { id: `c${n}`, name: name.trim(), params: effectsOf(params) }], lastVoice: `c${n}` };
}

/** Какой голос выбрали последним (`params.lastVoice`): его настройку можно перезаписать.
 *  Хранится в узле — значит, и в шаблоне; движок его не читает. */
export function lastVoice(params: Params): string | null {
  return typeof params.lastVoice === 'string' ? params.lastVoice : null;
}

/** Голос, который можно перезаписать: последний выбранный (свой или готовый, кроме «Обычного»),
 *  ползунки с тех пор изменены. */
export function overwritable(params: Params): ShownVoice | null {
  const last = lastVoice(params);
  if (last === null || last === KEEP_VOICE || voiceOf(params) !== null) return null;
  return voicesOf(params).find((v) => v.id === last) ?? null;
}

/** Перезаписать голос `id` текущей настройкой: свой — в списке своих (название и место прежние),
 *  готовый — в перезаписях этого узла. */
export function overwriteVoice(params: Params, id: string): Params {
  if (id === KEEP_VOICE) return {};
  if (VOICES.some((v) => v.id === id)) {
    return { voiceOverrides: { ...voiceOverrides(params), [id]: effectsOf(params) }, lastVoice: id };
  }
  return { voices: customVoices(params).map((v) => (v.id === id ? { ...v, params: effectsOf(params) } : v)), lastVoice: id };
}

/** Перезаписанный готовый голос `id` — как было в программе (и сразу выбрать его). */
export function resetVoice(params: Params, id: string): Params {
  const original = VOICES.find((v) => v.id === id);
  if (!original) return {};
  const { [id]: _gone, ...rest } = voiceOverrides(params);
  return { ...adapted(original.params), voiceOverrides: rest, lastVoice: id };
}

/** Убрать голос: свой — удаляется, готовый — прячется (его можно вернуть). */
export function removeVoice(params: Params, id: string): Params {
  if (id === KEEP_VOICE) return {};
  if (VOICES.some((v) => v.id === id)) return { hiddenVoices: [...new Set([...hiddenVoices(params), id])] };
  return { voices: customVoices(params).filter((v) => v.id !== id) };
}

/** Вернуть все убранные готовые голоса. */
export function restoreVoices(): Params {
  return { hiddenVoices: [] };
}

/** Название голоса для показа. */
export function voiceName(v: ShownVoice, t: (k: string) => string): string {
  return v.custom ? v.name : t(`voice.${v.id}`);
}
