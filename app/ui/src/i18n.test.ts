// SPDX-License-Identifier: AGPL-3.0-or-later
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { CATEGORY_ORDER, KIND_ORDER } from './graph';
import { DICTS, LANGUAGES, systemLanguage, translate } from './i18n';

function sourceFiles(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) return sourceFiles(path);
    return /\.tsx?$/.test(name) && !name.endsWith('.test.ts') ? [path] : [];
  });
}

describe('i18n', () => {
  it('all languages have the same keys', () => {
    const keys = Object.keys(DICTS.en).sort();
    for (const lang of LANGUAGES) expect(Object.keys(DICTS[lang]).sort()).toEqual(keys);
  });

  it('no empty translations', () => {
    for (const lang of LANGUAGES) for (const [k, v] of Object.entries(DICTS[lang])) expect(v.trim(), `${lang}:${k}`).not.toBe('');
  });

  it('every t("…") key used in the code exists', () => {
    const used = new Set<string>();
    for (const file of sourceFiles(join(__dirname))) {
      for (const m of readFileSync(file, 'utf8').matchAll(/\bt\(\s*'([\w.-]+)'/g)) used.add(m[1]);
    }
    expect(used.size).toBeGreaterThan(20);
    for (const key of used) expect(DICTS.en, key).toHaveProperty([key]);
  });

  it('every node kind and category has texts', () => {
    for (const kind of KIND_ORDER) {
      expect(DICTS.en).toHaveProperty([`node.${kind}.title`]);
      expect(DICTS.en).toHaveProperty([`node.${kind}.desc`]);
    }
    for (const cat of CATEGORY_ORDER) expect(DICTS.en).toHaveProperty([`category.${cat}`]);
    for (const lang of LANGUAGES) expect(DICTS.en).toHaveProperty([`lang.${lang}`]);
  });

  it('picks the system language or falls back to English', () => {
    expect(systemLanguage(['uk-UA', 'ru-RU'])).toBe('uk');
    expect(systemLanguage(['de-DE', 'ru'])).toBe('ru');
    expect(systemLanguage(['de-DE'])).toBe('en');
  });

  it('fills placeholders', () => {
    expect(translate('en', 'notice.recovered', { file: 'x.json' })).toContain('x.json');
  });
});
