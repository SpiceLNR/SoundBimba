// SPDX-License-Identifier: AGPL-3.0-or-later
import { fileURLToPath } from 'node:url';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vitest/config';

// Корень репозитория: темы (themes/) и переводы (locales/) лежат там, а не в app/.
const repoRoot = fileURLToPath(new URL('..', import.meta.url));

export default defineConfig({
  root: 'ui',
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    fs: { allow: [repoRoot] },
  },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    target: 'es2022',
  },
  test: {
    include: ['src/**/*.test.ts'],
  },
});
