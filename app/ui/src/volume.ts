// SPDX-License-Identifier: AGPL-3.0-or-later
// «Громкость» как на пульте: ползунок в децибелах (крайнее левое — выключено) и баланс.
// И «Выключить звук» на источниках и выходах (кнопка на узле).
import type { NodeKind, Params } from './graph';
import type { Translate } from './i18n';

/** Шкала ползунка, дБ. Левее нижней отметки — выключено; верх — как VOLUME_MAX_GAIN в Rust (×4). */
export const DB_MIN = -48;
export const DB_MAX = 12;
export const DB_STEP = 0.5;

/** У каких узлов на карте есть кнопка «Выключить звук». */
export const MUTABLE: ReadonlySet<NodeKind> = new Set<NodeKind>(['app-source', 'mic-input', 'sound-button', 'speaker-out', 'virtual-out']);

/** Положение ползунка (дБ) по усилению; выключено — DB_MIN. */
export function gainToDb(gain: number): number {
  if (!(gain > 0)) return DB_MIN;
  const db = Math.round((20 * Math.log10(gain)) / DB_STEP) * DB_STEP;
  return Math.min(DB_MAX, Math.max(DB_MIN + DB_STEP, db));
}

/** Усиление по положению ползунка (дБ); на нижней отметке — 0 (выключено). */
export function dbToGain(db: number): number {
  if (db <= DB_MIN) return 0;
  return Math.round(10 ** (db / 20) * 10000) / 10000;
}

/** Подпись громкости: «+3,5 дБ», «0 дБ», «выкл.». */
export function dbLabel(gain: number, t: Translate): string {
  const db = gainToDb(gain);
  if (db <= DB_MIN) return t('param.volumeOff');
  const num = String(Math.abs(db)).replace('.', t('num.decimal'));
  const sign = db > 0 ? '+' : db < 0 ? '−' : '';
  return `${sign}${num} ${t('unit.db')}`;
}

/** Подпись баланса: «по центру», «левее 30 %», «правее 100 %». */
export function balanceLabel(pan: number, t: Translate): string {
  const n = Math.round(Math.abs(pan) * 100);
  if (n === 0) return t('param.balanceCenter');
  return t(pan < 0 ? 'param.balanceLeft' : 'param.balanceRight', { n });
}

/**
 * Старые схемы (до 2026-11-13): у «Громкости» была галочка «Выключить звук». Её больше нет —
 * выключенная громкость открывается с ползунком на «выключено». Внутри микросхем — тоже.
 */
export function migrateParams(kind: NodeKind, params: Params): Params {
  if (kind === 'volume' && 'mute' in params) {
    const { mute, ...rest } = params;
    return mute === true ? { ...rest, gain: 0 } : rest;
  }
  if (kind === 'chip' && typeof params.inner === 'object' && params.inner !== null) {
    const inner = params.inner as { nodes?: unknown };
    if (Array.isArray(inner.nodes)) {
      const nodes = (inner.nodes as { type: NodeKind; params: Params }[]).map((n) => ({ ...n, params: migrateParams(n.type, n.params ?? {}) }));
      return { ...params, inner: { ...inner, nodes } };
    }
  }
  return params;
}
