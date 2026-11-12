// SPDX-License-Identifier: AGPL-3.0-or-later
// Готовые голоса морфера — сочетания пяти эффектов движка (crates/engine/src/morpher.rs):
// высота в полутонах (−12…12), «робот», «рация», эхо и дрожание высоты — сила 0…1.
import type { Params } from './graph';

export interface MorpherVoice {
  id: string;
  params: { pitch: number; robot: number; radio: number; echo: number; wobble: number };
}

export const VOICES: MorpherVoice[] = [
  { id: 'normal', params: { pitch: 0, robot: 0, radio: 0, echo: 0, wobble: 0 } },
  { id: 'monster', params: { pitch: -7, robot: 0, radio: 0, echo: 0, wobble: 0 } },
  { id: 'chipmunk', params: { pitch: 7, robot: 0, radio: 0, echo: 0, wobble: 0 } },
  { id: 'robot', params: { pitch: 0, robot: 1, radio: 0, echo: 0.2, wobble: 0 } },
  { id: 'radio', params: { pitch: 0, robot: 0, radio: 1, echo: 0, wobble: 0 } },
  { id: 'cave', params: { pitch: 0, robot: 0, radio: 0, echo: 0.8, wobble: 0 } },
  // «Плавающий» инопланетный голос: чуть выше, высота непрерывно дрожит, лёгкое эхо корабля.
  // (Первая версия — «чуть выше + робот» — автору не понравилась: смесь двух других голосов.)
  { id: 'alien', params: { pitch: 3, robot: 0, radio: 0, echo: 0.25, wobble: 0.7 } },
];

const KEYS = ['pitch', 'robot', 'radio', 'echo', 'wobble'] as const;
type Effects = MorpherVoice['params'];

/** Свой голос, сохранённый человеком. Хранится в самом узле морфера (`params.voices`) — поэтому
 *  попадает в проект и в шаблон вместе со схемой. */
export interface CustomVoice {
  id: string;
  name: string;
  params: Effects;
}

/** Голос в списке: готовый (название из перевода) или свой (название, которое дал человек). */
export type ShownVoice = { id: string; params: Effects } & ({ custom: false } | { custom: true; name: string });

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

/** Голоса в списке узла: готовые (кроме убранных), затем свои. */
export function voicesOf(params: Params): ShownVoice[] {
  const hidden = new Set(hiddenVoices(params));
  return [
    ...VOICES.filter((v) => v.id === KEEP_VOICE || !hidden.has(v.id)).map((v) => ({ ...v, custom: false as const })),
    ...customVoices(params).map((v) => ({ ...v, custom: true as const })),
  ];
}

/** Какой голос выбран (все значения совпадают), иначе null — своя настройка. */
export function voiceOf(params: Params): string | null {
  const now = effectsOf(params);
  return voicesOf(params).find((v) => KEYS.every((k) => now[k] === v.params[k]))?.id ?? null;
}

/** Сохранить текущую настройку как свой голос. */
export function addVoice(params: Params, name: string): Params {
  const list = customVoices(params);
  let n = list.length + 1;
  while (list.some((v) => v.id === `c${n}`)) n++;
  return { voices: [...list, { id: `c${n}`, name: name.trim(), params: effectsOf(params) }] };
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
