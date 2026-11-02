// SPDX-License-Identifier: AGPL-3.0-or-later
// Вызовы Rust-части приложения (команды Tauri). Сетевых запросов нет.
import { invoke } from '@tauri-apps/api/core';
import type { Project } from './graph';

export interface Settings {
  language: string | null;
  theme: string;
  /** Горячие клавиши: действие → сочетание («» — отключено). */
  hotkeys?: Record<string, string>;
}

export interface LoadState {
  project: Project;
  settings: Settings;
  recoveredFrom: string | null;
  version: string;
}

/** Ошибка из Rust: code — ключ причины (reason.<code>), detail — подробность для журнала. */
export interface CmdError {
  code: string;
  detail: string;
}

export function isCmdError(e: unknown): e is CmdError {
  return typeof e === 'object' && e !== null && 'code' in e && 'detail' in e;
}

// Только при разработке: интерфейс открыт в обычном браузере (без Tauri) — подставляем заглушку,
// чтобы проверить вид. В выпускную сборку этот код не попадает (import.meta.env.DEV = false).
const browserPreview = import.meta.env.DEV && !('__TAURI_INTERNALS__' in window);

const previewProject: Project = {
  schema: 1,
  nodes: [
    { id: 'n1', type: 'app-source', pos: [0, 0], params: { exe: '' } },
    { id: 'n2', type: 'mic-input', pos: [0, 160], params: { device: 'default' } },
    { id: 'n3', type: 'volume', pos: [260, 0], params: { gain: 1, mute: false } },
    { id: 'n4', type: 'virtual-out', pos: [520, 60], params: { device: 'virtual-1' } },
  ],
  edges: [
    { from: 'n1', to: 'n3' },
    { from: 'n2', to: 'n4' },
    { from: 'n3', to: 'n4' },
  ],
};

function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!browserPreview) return invoke<T>(cmd, args);
  if (cmd === 'load_state') {
    const state: LoadState = { project: previewProject, settings: { language: null, theme: 'bimba' }, recoveredFrom: null, version: 'dev' };
    return Promise.resolve(state as T);
  }
  return Promise.resolve(undefined as T);
}

export const api = {
  loadState: () => call<LoadState>('load_state'),
  saveProject: (project: Project) => call<void>('save_project', { project }),
  saveSettings: (settings: Settings) => call<void>('save_settings', { settings }),
  exportProject: (path: string, project: Project) => call<void>('export_project', { path, project }),
  importProject: (path: string) => call<Project>('import_project', { path }),
  saveDiagnostics: (path: string, uiInfo: string) => call<void>('save_diagnostics', { path, uiInfo }),
  log: (level: 'info' | 'error', message: string) => call<void>('log_ui', { level, message }).catch(() => {}),
};
