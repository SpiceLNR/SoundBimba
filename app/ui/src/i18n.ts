// SPDX-License-Identifier: AGPL-3.0-or-later
// Переводы: ключи из locales/{ru,en,uk}.json. Запасной язык — английский.
import { createContext, useContext } from 'react';
import en from '../../../locales/en.json';
import ru from '../../../locales/ru.json';
import uk from '../../../locales/uk.json';

export const LANGUAGES = ['ru', 'en', 'uk'] as const;
export type Lang = (typeof LANGUAGES)[number];
export type Translate = (key: string, vars?: Record<string, string | number>) => string;

export const DICTS: Record<Lang, Record<string, string>> = { ru, en, uk };

export function isLang(value: unknown): value is Lang {
  return typeof value === 'string' && (LANGUAGES as readonly string[]).includes(value);
}

/** Язык системы, если он среди наших; иначе английский. */
export function systemLanguage(preferred: readonly string[] = navigator.languages): Lang {
  for (const tag of preferred) {
    const code = tag.slice(0, 2).toLowerCase();
    if (isLang(code)) return code;
  }
  return 'en';
}

export function translate(lang: Lang, key: string, vars?: Record<string, string | number>): string {
  const text = DICTS[lang][key] ?? DICTS.en[key] ?? key;
  return vars ? text.replace(/\{(\w+)\}/g, (all, name: string) => String(vars[name] ?? all)) : text;
}

export const I18nContext = createContext<Translate>((key, vars) => translate('en', key, vars));

export function useT(): Translate {
  return useContext(I18nContext);
}
