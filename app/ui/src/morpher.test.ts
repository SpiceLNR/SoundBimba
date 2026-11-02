// SPDX-License-Identifier: AGPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { KINDS } from './graph';
import { DICTS, LANGUAGES } from './i18n';
import { VOICES, voiceOf } from './morpher';

describe('voice morpher', () => {
  it('a new node starts with the normal voice', () => {
    expect(voiceOf(KINDS['voice-morpher'].defaults)).toBe('normal');
    expect(voiceOf({})).toBe('normal');
  });

  it('recognizes voices and custom settings', () => {
    for (const v of VOICES) expect(voiceOf({ ...v.params })).toBe(v.id);
    expect(voiceOf({ pitch: -5, robot: 0, radio: 0, echo: 0, wobble: 0 })).toBeNull();
    expect(voiceOf({ wobble: 0.3 })).toBeNull();
    expect(new Set(VOICES.map((v) => JSON.stringify(v.params))).size).toBe(VOICES.length);
  });

  it('voices stay inside the allowed ranges', () => {
    // Те же пределы, что в crates/graph (MORPHER_RANGES).
    for (const { params: p } of VOICES) {
      expect(Math.abs(p.pitch)).toBeLessThanOrEqual(12);
      for (const k of ['robot', 'radio', 'echo', 'wobble'] as const) {
        expect(p[k]).toBeGreaterThanOrEqual(0);
        expect(p[k]).toBeLessThanOrEqual(1);
      }
    }
  });

  it('every voice has a name and hint in all languages', () => {
    for (const lang of LANGUAGES)
      for (const v of VOICES) {
        expect(DICTS[lang]).toHaveProperty([`voice.${v.id}`]);
        expect(DICTS[lang]).toHaveProperty([`voice.${v.id}.hint`]);
      }
  });
});
