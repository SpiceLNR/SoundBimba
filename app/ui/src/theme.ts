// SPDX-License-Identifier: AGPL-3.0-or-later
// Темы: папки themes/<id>/ с theme.json (названия, шрифты) и theme.css (переменные цвета и вида).
import type { Lang } from './i18n';

export interface ThemeMeta {
  id: string;
  names: Record<Lang, string>;
  fonts: string[];
  default?: boolean;
  /** Место в меню «Тема» (1 — первая). */
  order: number;
  /** Форма проводов: плавные кривые (по умолчанию), дорожки с прямыми углами, как на плате,
   *  или неоновые кривые с искрами у конца, когда провод моргает. */
  wires?: 'curve' | 'traces' | 'neon';
  /** Персонаж темы: картинка themes/<id>/mascot.svg и ключи фраз для облачка (по очереди). */
  mascot?: { lines: string[] };
}

// Стили всех тем подключаются сразу; выбранная включается атрибутом data-theme.
import.meta.glob('../../../themes/*/theme.css', { eager: true });
const metas = import.meta.glob<ThemeMeta>('../../../themes/*/theme.json', { eager: true, import: 'default' });

export const THEMES: ThemeMeta[] = Object.values(metas).sort((a, b) => a.order - b.order || a.id.localeCompare(b.id));

export const DEFAULT_THEME = THEMES.find((t) => t.default)?.id ?? 'bimba';

export function themeId(id: string | undefined): string {
  return THEMES.some((t) => t.id === id) ? (id as string) : DEFAULT_THEME;
}

/** Форма проводов выбранной темы. */
export function wireShape(id: string | undefined): 'curve' | 'traces' | 'neon' {
  return THEMES.find((t) => t.id === themeId(id))?.wires ?? 'curve';
}

export function applyTheme(id: string): void {
  document.documentElement.dataset.theme = themeId(id);
}
