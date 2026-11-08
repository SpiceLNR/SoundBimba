// SPDX-License-Identifier: AGPL-3.0-or-later
// Каждая тема задаёт все переменные вида: иначе в ней что-то «провалится» к значению браузера.
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { THEMES } from './theme';

const dir = join(__dirname, '../../../themes');
/** Переменные из главного блока темы `[data-theme='id'] { … }` (свои вспомогательные переменные
 *  внутри отдельных элементов, например у узлов, — не в счёт). */
const vars = (id: string) => {
  const css = readFileSync(join(dir, id, 'theme.css'), 'utf8');
  const start = css.indexOf(`[data-theme='${id}'] {`);
  const block = css.slice(start, css.indexOf('\n}', start));
  return new Set([...block.matchAll(/^\s*(--[a-z0-9-]+)\s*:/gm)].map((m) => m[1]));
};

describe('themes', () => {
  it('every theme folder is listed, with names in all languages and fonts', () => {
    const folders = readdirSync(dir).sort();
    expect(THEMES.map((t) => t.id).sort()).toEqual(folders);
    for (const t of THEMES) {
      expect(Object.keys(t.names).sort(), t.id).toEqual(['en', 'ru', 'uk']);
      expect(Number.isInteger(t.order), t.id).toBe(true);
      expect(t.fonts.length, t.id).toBeGreaterThan(0);
    }
  });

  it('themes go in the order the author chose', () => {
    expect(THEMES.map((t) => t.id)).toEqual(['bimba', 'metro', 'kids', 'kids-dark', 'neo-noir', 'magic', 'engineer', 'frutiger-aero']);
  });

  it('every theme defines the same variables as «Bimba»', () => {
    const base = vars('bimba');
    for (const t of THEMES) {
      const own = vars(t.id);
      expect([...base].filter((v) => !own.has(v)), `${t.id}: не хватает`).toEqual([]);
      expect([...own].filter((v) => !base.has(v)), `${t.id}: лишние`).toEqual([]);
    }
  });

  it('each theme scopes its variables to its own data-theme', () => {
    for (const t of THEMES) expect(readFileSync(join(dir, t.id, 'theme.css'), 'utf8')).toContain(`[data-theme='${t.id}']`);
  });
});
