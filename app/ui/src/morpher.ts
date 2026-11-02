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

/** Какой голос выбран (все значения совпадают), иначе null — своя настройка. */
export function voiceOf(params: Params): string | null {
  const value = (k: string) => (typeof params[k] === 'number' ? (params[k] as number) : 0);
  return VOICES.find((v) => KEYS.every((k) => value(k) === v.params[k]))?.id ?? null;
}
