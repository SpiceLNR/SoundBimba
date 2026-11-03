// SPDX-License-Identifier: AGPL-3.0-or-later
// Вызовы Rust-части приложения (команды и события Tauri). Сетевых запросов нет.
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { Project } from './graph';

export interface Settings {
  language: string | null;
  theme: string;
  /** Горячие клавиши: действие → сочетание («» — отключено). */
  hotkeys?: Record<string, string>;
  /** Звук включён (запоминается). */
  audioEnabled?: boolean;
  /** Рамка у окна кнопочной панели. */
  panelFrame?: boolean;
  /** Режим основного окна: карта («expert») или простой («simple»). */
  mode?: 'expert' | 'simple';
  /** Приветствие при первом запуске уже показано. */
  welcomeDone?: boolean;
  /** Встроенные шаблоны, убранные из списка. */
  hiddenTemplates?: string[];
}

/** Свой шаблон из папки пользователя (`id` — имя файла .bmb без расширения). */
export interface UserTemplate {
  id: string;
  name: string;
  description: string;
  project: Project;
}

/** Файлы .bmb, открытые двойным щелчком: что добавилось и коды ошибок того, что не открылось. */
export interface OpenedTemplates {
  templates: UserTemplate[];
  errors: string[];
}

export interface LoadState {
  project: Project;
  settings: Settings;
  recoveredFrom: string | null;
  version: string;
}

export interface AudioDevice {
  id: string;
  name: string;
  render: boolean;
  isDefault: boolean;
  /** Название виртуального кабеля, если это он. */
  cable: string | null;
}

export interface AudioApp {
  exe: string;
  active: boolean;
}

export interface Latency {
  outputMs: number;
  inputEstimateMs: number;
}

export type NodeStatus =
  | 'ok'
  | 'off'
  | 'app-not-chosen'
  | 'app-not-running'
  | 'device-missing'
  | 'no-cable'
  | 'error'
  | 'file-not-chosen'
  | 'file-missing'
  | 'file-unsupported';

/** Кнопка-звук: играет ли, позиция и длина в секундах. */
export interface ButtonPlayback {
  playing: boolean;
  pos: number;
  len: number;
}

/** Состояние звука, приходит из Rust ~15 раз в секунду. */
export interface AudioState {
  enabled: boolean;
  running: boolean;
  status: Record<string, NodeStatus>;
  levels: Record<string, number>;
  /** Кнопки со звуком: играет ли, позиция и длина. */
  buttons: Record<string, ButtonPlayback>;
  /** Компрессоры: насколько сжимают сейчас, дБ (нет записи — не сжимают). */
  reductions?: Record<string, number>;
  /** Шумоподавление: вероятность голоса 0…1 (нет записи — тишина). */
  voices?: Record<string, number>;
  /** «Оригинал звука»: выставлено в Windows, ждёт звука программы или ошибка. */
  routes?: Record<string, 'applied' | 'waiting' | 'error'>;
}

/** Проект и настройки без побочных действий (для кнопочной панели). */
export interface UiState {
  project: Project;
  settings: Settings;
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
    { id: 'n5', type: 'sound-button', pos: [0, 320], params: { file: 'D:/Sounds/fanfare.mp3' } },
  ],
  edges: [
    { from: 'n1', to: 'n3' },
    { from: 'n2', to: 'n4' },
    { from: 'n3', to: 'n4' },
    { from: 'n5', to: 'n4' },
  ],
};

const previewDevices: AudioDevice[] = [
  { id: 'spk', name: 'Speakers (Realtek(R) Audio)', render: true, isDefault: true, cable: null },
  { id: 'cab', name: 'CABLE Input (VB-Audio Virtual Cable)', render: true, isDefault: false, cable: 'VB-CABLE' },
  { id: 'mic', name: 'Microphone (USB Audio CODEC)', render: false, isDefault: true, cable: null },
  { id: 'cabo', name: 'CABLE Output (VB-Audio Virtual Cable)', render: false, isDefault: false, cable: 'VB-CABLE' },
];

let previewEnabled = false;
const previewTemplates: UserTemplate[] = [];
const previewPlaying: Record<string, ButtonPlayback> = {};

function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!browserPreview) return invoke<T>(cmd, args);
  const answers: Record<string, unknown> = {
    load_state: { project: previewProject, settings: { language: null, theme: 'bimba' }, recoveredFrom: null, version: 'dev' },
    list_audio_devices: previewDevices,
    measure_latency: { outputMs: 41.3, inputEstimateMs: 25 },
    list_audio_apps: [
      { exe: 'chrome.exe', active: true },
      { exe: 'Discord.exe', active: false },
    ],
  };
  if (cmd === 'set_audio_enabled') previewEnabled = Boolean(args?.enabled);
  if (cmd === 'list_templates') return Promise.resolve([...previewTemplates] as T);
  if (cmd === 'save_template') {
    const t = { id: `t${previewTemplates.length + 1}`, name: String(args?.name).trim(), description: String(args?.description).trim(), project: args?.project as Project };
    previewTemplates.push(t);
    return Promise.resolve(t as T);
  }
  if (cmd === 'delete_template') previewTemplates.splice(previewTemplates.findIndex((t) => t.id === args?.id), 1);
  if (cmd === 'take_opened_templates') return Promise.resolve({ templates: [], errors: [] } as T);
  if (cmd === 'press_button') {
    const id = String(args?.nodeId);
    const b = (previewPlaying[id] ??= { playing: false, pos: 0, len: 95 });
    b.playing = !b.playing; // как в программе: повторное нажатие — пауза, позиция остаётся
  }
  if (cmd === 'seek_button') {
    const b = (previewPlaying[String(args?.nodeId)] ??= { playing: false, pos: 0, len: 95 });
    b.playing = true;
    b.pos = Number(args?.fraction) * b.len;
  }
  if (cmd === 'get_ui_state') return Promise.resolve({ project: previewProject, settings: { language: null, theme: 'bimba' } } as T);
  return Promise.resolve(answers[cmd] as T);
}

/** Подписка на состояние звука. Возвращает функцию отписки. */
export function onAudioState(handler: (s: AudioState) => void): () => void {
  if (browserPreview) {
    // Заглушка: «живые» уровни, чтобы увидеть индикаторы в браузере.
    const h = setInterval(() => {
      const levels: Record<string, number> = {};
      for (const n of previewProject.nodes) levels[n.id] = previewEnabled ? Math.random() * 0.6 : 0;
      const status: Record<string, NodeStatus> = {};
      for (const n of previewProject.nodes) status[n.id] = previewEnabled ? 'ok' : 'off';
      if (previewEnabled) status.n1 = 'app-not-chosen';
      for (const b of Object.values(previewPlaying)) {
        if (!b.playing) continue;
        b.pos += 0.066;
        if (b.pos >= b.len) {
          b.playing = false;
          b.pos = 0;
        }
      }
      if (previewEnabled) previewPlaying.n5 ??= { playing: false, pos: 0, len: 95 };
      const buttons: Record<string, ButtonPlayback> = {};
      for (const [id, b] of Object.entries(previewPlaying)) buttons[id] = { ...b };
      const reductions: Record<string, number> = {};
      for (const n of previewProject.nodes) if (n.type === 'compressor' && previewEnabled) reductions[n.id] = Math.random() * 9;
      handler({ enabled: previewEnabled, running: previewEnabled, status, levels, buttons, reductions });
    }, 66);
    return () => clearInterval(h);
  }
  let unlisten: (() => void) | undefined;
  let cancelled = false;
  void listen<AudioState>('audio-state', (e) => handler(e.payload)).then((u) => {
    if (cancelled) u();
    else unlisten = u;
  });
  return () => {
    cancelled = true;
    unlisten?.();
  };
}

/** Файлы .bmb, открытые двойным щелчком, пока программа уже работала. Возвращает отписку. */
export function onTemplatesOpened(handler: (o: OpenedTemplates) => void): () => void {
  if (browserPreview) return () => {};
  let unlisten: (() => void) | undefined;
  let cancelled = false;
  void listen<OpenedTemplates>('templates-opened', (e) => handler(e.payload)).then((u) => {
    if (cancelled) u();
    else unlisten = u;
  });
  return () => {
    cancelled = true;
    unlisten?.();
  };
}

export const api = {
  loadState: () => call<LoadState>('load_state'),
  saveProject: (project: Project) => call<void>('save_project', { project }),
  saveSettings: (settings: Settings) => call<void>('save_settings', { settings }),
  exportProject: (path: string, project: Project) => call<void>('export_project', { path, project }),
  importProject: (path: string) => call<Project>('import_project', { path }),
  listTemplates: () => call<UserTemplate[]>('list_templates'),
  saveTemplate: (name: string, description: string, project: Project) =>
    call<UserTemplate>('save_template', { name, description, project }),
  deleteTemplate: (id: string) => call<void>('delete_template', { id }),
  importTemplate: (path: string) => call<UserTemplate>('import_template', { path }),
  exportTemplate: (id: string, path: string) => call<void>('export_template', { id, path }),
  takeOpenedTemplates: () => call<OpenedTemplates>('take_opened_templates'),
  saveDiagnostics: (path: string, uiInfo: string) => call<void>('save_diagnostics', { path, uiInfo }),
  setAudioEnabled: (enabled: boolean) => call<void>('set_audio_enabled', { enabled }),
  listAudioDevices: () => call<AudioDevice[]>('list_audio_devices'),
  listAudioApps: () => call<AudioApp[]>('list_audio_apps'),
  measureLatency: () => call<Latency>('measure_latency'),
  pressButton: (nodeId: string) => call<void>('press_button', { nodeId }),
  seekButton: (nodeId: string, fraction: number) => call<void>('seek_button', { nodeId, fraction }),
  getUiState: () => call<UiState>('get_ui_state'),
  log: (level: 'info' | 'error', message: string) => call<void>('log_ui', { level, message }).catch(() => {}),
};
