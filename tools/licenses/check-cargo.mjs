// SPDX-License-Identifier: AGPL-3.0-or-later
// Проверка лицензий Rust-зависимостей для Windows (по cargo metadata).
// Запуск из корня репозитория: node tools/licenses/check-cargo.mjs
import { execFileSync } from 'node:child_process';
import { report } from './allowed.mjs';

const meta = JSON.parse(
  execFileSync('cargo', ['metadata', '--format-version', '1', '--filter-platform', 'x86_64-pc-windows-msvc'], {
    encoding: 'utf8',
    maxBuffer: 256 * 1024 * 1024,
  }),
);
const used = new Set(meta.resolve.nodes.map((n) => n.id));
const items = meta.packages
  .filter((p) => used.has(p.id) && p.source) // только сторонние (у своих крейтов source = null)
  .map((p) => ({ name: p.name, version: p.version, license: p.license }));
report('cargo', items);
