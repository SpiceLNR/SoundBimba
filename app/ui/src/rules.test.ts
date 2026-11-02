// SPDX-License-Identifier: AGPL-3.0-or-later
// Автопроверка правил проекта (CLAUDE.md): цвет только из тем, офлайн, SPDX-заголовки.
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

function files(dir: string, re: RegExp): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) return files(path, re);
    return re.test(name) ? [path] : [];
  });
}

const src = join(__dirname);
const code = files(src, /\.(tsx?|css)$/).filter((f) => !f.endsWith('.test.ts'));

describe('project rules', () => {
  it('no colors outside themes', () => {
    const color = /#[0-9a-fA-F]{3,8}\b|\brgba?\(|\bhsla?\(/;
    for (const f of code) {
      const text = readFileSync(f, 'utf8');
      expect(color.test(text), f).toBe(false);
    }
  });

  it('no network addresses in the interface', () => {
    for (const f of code) expect(/https?:\/\//.test(readFileSync(f, 'utf8')), f).toBe(false);
  });

  it('every source file has an SPDX header', () => {
    for (const f of files(src, /\.(tsx?|css)$/)) {
      expect(readFileSync(f, 'utf8').slice(0, 200), f).toContain('SPDX-License-Identifier: AGPL-3.0-or-later');
    }
  });
});
