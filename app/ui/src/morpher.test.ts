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

describe('own voices', () => {
  it('saves the current setting as a voice, picks it, removes it', async () => {
    const { addVoice, removeVoice, voiceOf, voicesOf } = await import('./morpher');
    let p: Record<string, unknown> = { pitch: -3, robot: 0.4, radio: 0, echo: 0.1, wobble: 0 };
    expect(voiceOf(p)).toBeNull();
    p = { ...p, ...addVoice(p, '  Мой бас  ') };
    const mine = voicesOf(p).at(-1)!;
    expect(mine).toMatchObject({ id: 'c1', custom: true, name: 'Мой бас' });
    expect(voiceOf(p)).toBe('c1');
    // Второй свой голос получает новый номер, даже если первый удалили.
    p = { ...p, ...addVoice({ ...p, pitch: 5 }, 'Писк') };
    p = { ...p, ...removeVoice(p, 'c1') };
    expect(voicesOf(p).filter((v) => v.custom).map((v) => v.id)).toEqual(['c2']);
    p = { ...p, ...addVoice(p, 'Ещё') };
    expect(voicesOf(p).filter((v) => v.custom).map((v) => v.id)).toEqual(['c2', 'c3']);
  });

  it('built-in voices are hidden, not lost, and come back; "normal" always stays', async () => {
    const { removeVoice, restoreVoices, voicesOf, VOICES } = await import('./morpher');
    let p: Record<string, unknown> = {};
    p = { ...p, ...removeVoice(p, 'alien') };
    p = { ...p, ...removeVoice(p, 'robot') };
    expect(removeVoice(p, 'normal')).toEqual({});
    expect(voicesOf(p).map((v) => v.id)).toEqual(VOICES.map((v) => v.id).filter((id) => id !== 'alien' && id !== 'robot'));
    p = { ...p, ...restoreVoices() };
    expect(voicesOf(p).map((v) => v.id)).toEqual(VOICES.map((v) => v.id));
    // «Пришелец» после возврата — тот же самый.
    expect(voicesOf(p).find((v) => v.id === 'alien')!.params).toEqual({ pitch: 3, robot: 0, radio: 0, echo: 0.25, wobble: 0.7, formant: 0, growl: 0, chorus: 0, hall: 0, tone: 0, lowcut: 0, bright: 0 });
  });

  it('overwrites the last picked own voice with the current sliders', async () => {
    const { addVoice, overwritable, overwriteVoice, voiceOf, voicesOf } = await import('./morpher');
    let p: Record<string, unknown> = { pitch: -3, robot: 0.4 };
    p = { ...p, ...addVoice(p, 'Мой бас') };
    p = { ...p, ...addVoice({ ...p, pitch: 6 }, 'Писк'), pitch: 6 };
    // Выбрали «Мой бас» (как кнопка в списке) — ползунки те же, перезаписывать нечего.
    p = { ...p, ...voicesOf(p).find((v) => v.id === 'c1')!.params, lastVoice: 'c1' };
    expect(overwritable(p)).toBeNull();
    // Подвинули ползунок — можно перезаписать «Мой бас».
    p = { ...p, echo: 0.5 };
    expect(overwritable(p)).toMatchObject({ id: 'c1', custom: true, name: 'Мой бас' });
    p = { ...p, ...overwriteVoice(p, 'c1') };
    expect(voiceOf(p)).toBe('c1');
    expect(overwritable(p)).toBeNull();
    const own = voicesOf(p).filter((v) => v.custom);
    // Название и порядок прежние, второй голос не тронут.
    expect(own.map((v) => [v.id, v.name])).toEqual([['c1', 'Мой бас'], ['c2', 'Писк']]);
    expect(own[0].params).toMatchObject({ pitch: -3, robot: 0.4, echo: 0.5 });
    expect(own[1].params).toMatchObject({ pitch: 6, robot: 0.4, echo: 0 });
  });

  it('a built-in voice is overwritten only in this node and can be reset', async () => {
    const { overwritable, overwriteVoice, resetVoice, voiceOf, voicesOf, VOICES } = await import('./morpher');
    const original = VOICES.find((v) => v.id === 'alien')!.params;
    // Выбрали «Пришельца», подвинули ползунок — можно перезаписать.
    let p: Record<string, unknown> = { ...original, lastVoice: 'alien', echo: 0.9 };
    expect(overwritable(p)?.id).toBe('alien');
    p = { ...p, ...overwriteVoice(p, 'alien') };
    expect(voiceOf(p)).toBe('alien');
    const shown = voicesOf(p).find((v) => v.id === 'alien')!;
    expect(shown).toMatchObject({ custom: false, changed: true, params: { ...original, echo: 0.9 } });
    // В программе «Пришелец» прежний; другой узел его перезаписи не видит.
    expect(VOICES.find((v) => v.id === 'alien')!.params).toEqual(original);
    expect(voicesOf({}).find((v) => v.id === 'alien')!.params).toEqual(original);
    // «Вернуть как было»: голос снова прежний и сразу выбран.
    p = { ...p, ...resetVoice(p, 'alien') };
    expect(voicesOf(p).find((v) => v.id === 'alien')).toMatchObject({ changed: false, params: original });
    expect(voiceOf(p)).toBe('alien');
  });

  it('"normal" and removed voices cannot be overwritten', async () => {
    const { overwritable, overwriteVoice, removeVoice, voiceOverrides } = await import('./morpher');
    // «Обычный» — путь обратно к исходному голосу, его не перезаписать.
    const normal = { lastVoice: 'normal', echo: 0.5 };
    expect(overwritable(normal)).toBeNull();
    expect(overwriteVoice(normal, 'normal')).toEqual({});
    expect(voiceOverrides({ voiceOverrides: { normal: { echo: 1 }, nope: { echo: 1 }, cave: 5 } })).toEqual({});
    // Свой голос убрали — перезаписывать некого.
    let p: Record<string, unknown> = { voices: [{ id: 'c1', name: 'А', params: { pitch: 1 } }], lastVoice: 'c1', pitch: 2 };
    expect(overwritable(p)?.id).toBe('c1');
    p = { ...p, ...removeVoice(p, 'c1') };
    expect(overwritable(p)).toBeNull();
  });

  it('broken voice entries in a file are skipped', async () => {
    const { voicesOf } = await import('./morpher');
    const p = { voices: [null, 5, { id: 'c1' }, { id: 'c2', name: 'Ок', params: { pitch: 2, robot: 'x' } }] };
    const own = voicesOf(p).filter((v) => v.custom);
    expect(own).toEqual([{ id: 'c2', name: 'Ок', custom: true, params: { pitch: 2, robot: 0, radio: 0, echo: 0, wobble: 0, formant: 0, growl: 0, chorus: 0, hall: 0, tone: 0, lowcut: 0, bright: 0 } }]);
  });
});

describe('tune to your voice', () => {
  it('ready voices reach the same pitch from any voice, without flipping direction', async () => {
    const { adapted, VOICES, REFERENCE_F0 } = await import('./morpher');
    const girl = VOICES.find((v) => v.id === 'girl')!.params;
    const demon = VOICES.find((v) => v.id === 'demon')!.params;
    // Голос автора (на нём настроены голоса) и «не подстроено» — без изменений.
    expect(adapted(girl, REFERENCE_F0)).toEqual(girl);
    expect(adapted(girl, null)).toEqual(girl);
    // Низкий мужской голос: «Девушка» поднимает сильнее, тембр — тоже легче.
    const deep = adapted(girl, 100);
    expect(deep.tone).toBe(9);
    expect(deep.formant).toBe(4);
    // Высокий голос: «Девушка» не опускает — голос как есть.
    expect(adapted(girl, 210)).toMatchObject({ tone: 0, formant: 0 });
    // «Демон» у высокого голоса — ниже, вплоть до предела.
    const high = adapted(demon, 210);
    expect(high.tone).toBe(-12);
    expect(high.formant).toBeLessThan(demon.formant);
    // Мультяшные голоса (без естественной высоты) не трогаем.
    const alien = VOICES.find((v) => v.id === 'alien')!.params;
    expect(adapted(alien, 210)).toEqual(alien);
  });

  it('the voice picker shows tuned voices as chosen', async () => {
    const { adapted, setVoiceProfile, voiceOf, VOICES } = await import('./morpher');
    setVoiceProfile(100);
    const girl = adapted(VOICES.find((v) => v.id === 'girl')!.params, 100);
    expect(voiceOf(girl)).toBe('girl');
    setVoiceProfile(null);
  });
});
