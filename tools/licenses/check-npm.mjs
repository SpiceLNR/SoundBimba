// SPDX-License-Identifier: AGPL-3.0-or-later
// Проверка лицензий npm-пакетов интерфейса (app/node_modules, по package-lock.json).
// Запуск из папки app: npm run licenses. Dev-пакеты (сборка, тесты) проверяются тоже.
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { report } from './allowed.mjs';

const appDir = process.cwd();
const lock = JSON.parse(readFileSync(join(appDir, 'package-lock.json'), 'utf8'));
const items = [];
for (const [path, info] of Object.entries(lock.packages)) {
  if (!path.startsWith('node_modules/')) continue;
  let license = info.license;
  if (!license) {
    try {
      license = JSON.parse(readFileSync(join(appDir, path, 'package.json'), 'utf8')).license;
    } catch {
      // Пакета нет на этой платформе (необязательная зависимость для другой ОС).
      continue;
    }
  }
  items.push({ name: path.slice(path.lastIndexOf('node_modules/') + 13), version: info.version, license });
}
report('npm', items);
