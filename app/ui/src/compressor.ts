// SPDX-License-Identifier: AGPL-3.0-or-later
// Готовые наборы компрессора. Значения — в тех же единицах, что в проекте (см. crates/graph:
// COMPRESSOR_RANGES): порог и усиление в дБ, сила — «во сколько раз», атака и восстановление в мс.
import type { Params } from './graph';

export interface CompressorPreset {
  id: string;
  params: { threshold: number; ratio: number; attack: number; release: number; makeup: number };
}

export const COMPRESSOR_PRESETS: CompressorPreset[] = [
  // Замер на микрофоне автора (речь в среднем −32 дБ, пики до −1 дБ, 2026-11-03): с прежним
  // усилением 6–12 дБ голос становился громче лишь на 3–4 дБ — на слух незаметно. Теперь:
  // Ровнее и громче (≈ +10 дБ): значение по умолчанию у нового узла.
  { id: 'voice', params: { threshold: -24, ratio: 3, attack: 5, release: 120, makeup: 12 } },
  // Сильно поднимает тихую речь (≈ +14 дБ), громкое не даёт перегрузить.
  { id: 'louder', params: { threshold: -40, ratio: 4, attack: 3, release: 200, makeup: 20 } },
  // Плотно и громко, «как на радио» (≈ +16 дБ).
  { id: 'strong', params: { threshold: -30, ratio: 8, attack: 1, release: 80, makeup: 22 } },
];

/** Число с настоящим минусом (−24, а не -24). */
export function signed(v: number): string {
  return v < 0 ? `−${-v}` : String(v);
}

/** Какой набор сейчас выбран (все пять значений совпадают), иначе null — настроено вручную. */
export function presetOf(params: Params): string | null {
  const found = COMPRESSOR_PRESETS.find((pr) =>
    (Object.keys(pr.params) as (keyof CompressorPreset['params'])[]).every((k) => params[k] === pr.params[k]),
  );
  return found?.id ?? null;
}
