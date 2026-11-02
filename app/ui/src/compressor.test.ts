// SPDX-License-Identifier: AGPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { COMPRESSOR_PRESETS, presetOf } from './compressor';
import { KINDS } from './graph';
import { DICTS, LANGUAGES } from './i18n';

describe('compressor', () => {
  it('a new node starts with the "voice" preset', () => {
    expect(presetOf(KINDS.compressor.defaults)).toBe('voice');
  });

  it('recognizes presets and manual settings', () => {
    for (const pr of COMPRESSOR_PRESETS) expect(presetOf({ ...pr.params })).toBe(pr.id);
    expect(presetOf({ ...COMPRESSOR_PRESETS[0].params, ratio: 5 })).toBeNull();
  });

  it('presets stay inside the allowed ranges', () => {
    // Те же пределы, что в crates/graph (COMPRESSOR_RANGES).
    for (const { params: p } of COMPRESSOR_PRESETS) {
      expect(p.threshold).toBeGreaterThanOrEqual(-60);
      expect(p.threshold).toBeLessThanOrEqual(0);
      expect(p.ratio).toBeGreaterThanOrEqual(1);
      expect(p.ratio).toBeLessThanOrEqual(20);
      expect(p.attack).toBeGreaterThanOrEqual(0.1);
      expect(p.attack).toBeLessThanOrEqual(100);
      expect(p.release).toBeGreaterThanOrEqual(10);
      expect(p.release).toBeLessThanOrEqual(2000);
      expect(p.makeup).toBeGreaterThanOrEqual(0);
      expect(p.makeup).toBeLessThanOrEqual(24);
    }
  });

  it('every preset has a name and hint in all languages', () => {
    for (const lang of LANGUAGES)
      for (const pr of COMPRESSOR_PRESETS) {
        expect(DICTS[lang]).toHaveProperty([`preset.${pr.id}`]);
        expect(DICTS[lang]).toHaveProperty([`preset.${pr.id}.hint`]);
      }
  });
});
