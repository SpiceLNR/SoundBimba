// SPDX-License-Identifier: AGPL-3.0-or-later
// Горячие клавиши — только внутри окна SoundBimba (правило 6). Все переназначаются в окне «Клавиши».
// Сочетание хранится по физической клавише (KeyboardEvent.code), чтобы работать в любой раскладке:
// «Ctrl+KeyL» срабатывает и в английской, и в русской («Ctrl+Д»).

export const ACTIONS = ['deleteSelected', 'deselect', 'arrange', 'fitView', 'export', 'import', 'diagnostics'] as const;
export type Action = (typeof ACTIONS)[number];
/** Действие → сочетание; пустая строка — клавиша отключена. */
export type Bindings = Record<Action, string>;

export const DEFAULT_BINDINGS: Bindings = {
  deleteSelected: 'Delete',
  deselect: 'Escape',
  arrange: 'Ctrl+KeyL',
  fitView: 'Ctrl+Digit0',
  export: 'Ctrl+KeyS',
  import: 'Ctrl+KeyO',
  diagnostics: '',
};

const MODIFIERS = new Set(['ControlLeft', 'ControlRight', 'ShiftLeft', 'ShiftRight', 'AltLeft', 'AltRight', 'MetaLeft', 'MetaRight']);

type KeyInfo = Pick<KeyboardEvent, 'code' | 'ctrlKey' | 'shiftKey' | 'altKey' | 'metaKey'>;

/** Сочетание из нажатия, например «Ctrl+Shift+KeyL». null — нажат только модификатор. */
export function eventToCombo(e: KeyInfo): string | null {
  if (!e.code || MODIFIERS.has(e.code)) return null;
  const parts: string[] = [];
  if (e.ctrlKey || e.metaKey) parts.push('Ctrl');
  if (e.altKey) parts.push('Alt');
  if (e.shiftKey) parts.push('Shift');
  parts.push(e.code);
  return parts.join('+');
}

// Подписи клавиш — как на клавиатуре.
const KEY_LABELS: Record<string, string> = {
  Escape: 'Esc',
  ArrowUp: '↑',
  ArrowDown: '↓',
  ArrowLeft: '←',
  ArrowRight: '→',
  Minus: '-',
  Equal: '=',
  BracketLeft: '[',
  BracketRight: ']',
  Semicolon: ';',
  Quote: "'",
  Comma: ',',
  Period: '.',
  Slash: '/',
  Backslash: '\\',
  Backquote: '`',
};

export function formatCombo(combo: string): string {
  return combo
    .split('+')
    .map((p) => {
      if (p.startsWith('Key')) return p.slice(3);
      if (p.startsWith('Digit')) return p.slice(5);
      if (p.startsWith('Numpad')) return `Num ${p.slice(6)}`;
      return KEY_LABELS[p] ?? p;
    })
    .join(' + ');
}

/** Сохранённые настройки поверх значений по умолчанию (новые действия получают своё сочетание). */
export function mergeBindings(saved?: Record<string, string>): Bindings {
  const b = { ...DEFAULT_BINDINGS };
  for (const a of ACTIONS) if (typeof saved?.[a] === 'string') b[a] = saved[a];
  return b;
}

/** Назначает сочетание; если оно было у другого действия, там оно отключается. */
export function assign(b: Bindings, action: Action, combo: string): { bindings: Bindings; displaced?: Action } {
  const displaced = combo ? ACTIONS.find((a) => a !== action && b[a] === combo) : undefined;
  const bindings = { ...b, [action]: combo };
  if (displaced) bindings[displaced] = '';
  return { bindings, displaced };
}

export function actionFor(b: Bindings, combo: string): Action | undefined {
  return ACTIONS.find((a) => b[a] !== '' && b[a] === combo);
}

const TEXT_INPUTS = new Set(['text', 'search', 'number', 'email', 'password', 'url', 'tel']);

/** Человек печатает текст — тогда клавиши программы не срабатывают (Delete стирает буквы). */
export function isTyping(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable || target.tagName === 'TEXTAREA') return true;
  return target instanceof HTMLInputElement && TEXT_INPUTS.has(target.type);
}
