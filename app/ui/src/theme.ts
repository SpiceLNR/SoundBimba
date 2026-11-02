// SPDX-License-Identifier: AGPL-3.0-or-later
// Темы: папки themes/<id>/ с theme.json (названия, шрифты) и theme.css (переменные цвета и вида).
import type { Lang } from './i18n';

export interface ThemeMeta {
  id: string;
  names: Record<Lang, string>;
  fonts: string[];
  default?: boolean;
}

// Стили всех тем подключаются сразу; выбранная включается атрибутом data-theme.
import.meta.glob('../../../themes/*/theme.css', { eager: true });
const metas = import.meta.glob<ThemeMeta>('../../../themes/*/theme.json', { eager: true, import: 'default' });

export const THEMES: ThemeMeta[] = Object.values(metas).sort(
  (a, b) => Number(Boolean(b.default)) - Number(Boolean(a.default)) || a.id.localeCompare(b.id),
);

export const DEFAULT_THEME = THEMES.find((t) => t.default)?.id ?? 'bimba';

export function themeId(id: string | undefined): string {
  return THEMES.some((t) => t.id === id) ? (id as string) : DEFAULT_THEME;
}

export function applyTheme(id: string): void {
  document.documentElement.dataset.theme = themeId(id);
}
