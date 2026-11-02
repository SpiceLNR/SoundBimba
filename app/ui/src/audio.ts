// SPDX-License-Identifier: AGPL-3.0-or-later
// Состояние звука для компонентов: включён ли, что с каждым узлом, уровни.
import { createContext, useContext } from 'react';
import type { AudioState, NodeStatus } from './api';

export const EMPTY_AUDIO: AudioState = { enabled: false, running: false, status: {}, levels: {} };

export const AudioContext = createContext<AudioState>(EMPTY_AUDIO);

export function useAudio(): AudioState {
  return useContext(AudioContext);
}

/** Уровень (пик 0…1) → доля шкалы по децибелам: −60 дБ — пусто, 0 дБ — полная. */
export function meterFraction(peak: number): number {
  if (!(peak > 0)) return 0;
  const db = 20 * Math.log10(peak);
  return Math.min(1, Math.max(0, (db + 60) / 60));
}

/** Состояния, о которых узел должен сказать вслух (вместо обычной подписи). */
export function isProblem(s: NodeStatus | undefined): s is Exclude<NodeStatus, 'ok' | 'off'> {
  return s !== undefined && s !== 'ok' && s !== 'off';
}
