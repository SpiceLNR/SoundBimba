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
