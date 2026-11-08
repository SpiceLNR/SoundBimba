// SPDX-License-Identifier: AGPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { KIND_ORDER } from './graph';
import { ACTIONS, actionFor, actionLabel, addedKind, assign, DEFAULT_BINDINGS, eventToCombo, formatCombo, mergeBindings } from './hotkeys';
import { DICTS, translate } from './i18n';

const key = (code: string, mods: Partial<{ ctrlKey: boolean; shiftKey: boolean; altKey: boolean; metaKey: boolean }> = {}) => ({
  code,
  ctrlKey: false,
  shiftKey: false,
  altKey: false,
  metaKey: false,
  ...mods,
});

describe('hotkeys', () => {
  it('builds combos by physical key, layout independent', () => {
    expect(eventToCombo(key('Delete'))).toBe('Delete');
    expect(eventToCombo(key('KeyL', { ctrlKey: true, shiftKey: true }))).toBe('Ctrl+Shift+KeyL');
    expect(eventToCombo(key('ControlLeft', { ctrlKey: true }))).toBeNull();
  });

  it('shows combos like on the keyboard', () => {
    expect(formatCombo('Ctrl+KeyL')).toBe('Ctrl + L');
    expect(formatCombo('Ctrl+Digit0')).toBe('Ctrl + 0');
    expect(formatCombo('Escape')).toBe('Esc');
  });

  it('finds the action for a combo, ignoring disabled ones', () => {
    expect(actionFor(DEFAULT_BINDINGS, 'Delete')).toBe('deleteSelected');
    expect(actionFor(DEFAULT_BINDINGS, '')).toBeUndefined();
    expect(actionFor({ ...DEFAULT_BINDINGS, deleteSelected: '' }, 'Delete')).toBeUndefined();
  });

  it('moving a combo to another action frees it there', () => {
    const { bindings, displaced } = assign(DEFAULT_BINDINGS, 'diagnostics', 'Ctrl+KeyS');
    expect(displaced).toBe('export');
    expect(bindings.diagnostics).toBe('Ctrl+KeyS');
    expect(bindings.export).toBe('');
  });

  it('keeps saved choices and adds defaults for new actions', () => {
    const b = mergeBindings({ deleteSelected: 'Backspace', arrange: '', unknown: 'X' });
    expect(b.deleteSelected).toBe('Backspace');
    expect(b.arrange).toBe('');
    expect(b.fitView).toBe(DEFAULT_BINDINGS.fitView);
    expect(b).not.toHaveProperty('unknown');
  });

  it('add-node digits follow the order of the "Add node" list', () => {
    const digits = ACTIONS.filter((a) => addedKind(a)).map((a) => DEFAULT_BINDINGS[a]);
    // 1–9 — по списку, 0 — микросхема; ножки (только внутри микросхемы) — без клавиш.
    expect(digits).toEqual(['Digit1', 'Digit2', 'Digit3', 'Digit4', 'Digit5', 'Digit6', 'Digit7', 'Digit8', 'Digit9', 'Digit0', '', '']);
  });

  it('the sound button is first: digit 1, old saved 1–3 move along', () => {
    expect(DEFAULT_BINDINGS['add-sound-button']).toBe('Digit1');
    // Сохранено, как было до 2026-11-08: приложение 1, микрофон 2, кнопка 3.
    const b = mergeBindings({ 'add-app-source': 'Digit1', 'add-mic-input': 'Digit2', 'add-sound-button': 'Digit3' });
    expect(b['add-sound-button']).toBe('Digit1');
    expect(b['add-app-source']).toBe('Digit2');
    expect(b['add-mic-input']).toBe('Digit3');
  });

  it('old saved defaults move to the new digits, own choices stay', () => {
    // Сохранено, как было раньше: наушники 5, виртуальный микрофон 6 — и своя клавиша у компрессора.
    const b = mergeBindings({ 'add-speaker-out': 'Digit5', 'add-virtual-out': 'Digit6', 'add-compressor': 'KeyC', 'add-volume': 'Digit4' });
    expect(b['add-speaker-out']).toBe('Digit8');
    expect(b['add-virtual-out']).toBe('Digit9');
    expect(b['add-compressor']).toBe('KeyC');
    expect(b['add-volume']).toBe('Digit4');
    // Своё сочетание, совпавшее с чужим по умолчанию, побеждает — без двойных назначений.
    const c = mergeBindings({ arrange: 'Digit5' });
    expect(c.arrange).toBe('Digit5');
    expect(c['add-compressor']).toBe('');
  });

  it('default combos are unique and every action has a name', () => {
    const used = ACTIONS.map((a) => DEFAULT_BINDINGS[a]).filter(Boolean);
    expect(new Set(used).size).toBe(used.length);
    for (const a of ACTIONS) if (!addedKind(a)) expect(DICTS.en).toHaveProperty([`action.${a}`]);
  });

  it('has an add action with a readable name for every node kind', () => {
    for (const k of KIND_ORDER) {
      expect(ACTIONS).toContain(`add-${k}`);
      expect(addedKind(`add-${k}`)).toBe(k);
    }
    expect(actionLabel('add-volume', (k, v) => translate('ru', k, v))).toBe('Добавить: Громкость');
    expect(actionFor(DEFAULT_BINDINGS, 'Digit4')).toBe('add-volume');
  });
});
