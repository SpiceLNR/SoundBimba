// SPDX-License-Identifier: AGPL-3.0-or-later
// Морфер голоса: выбор голоса (он же — в простом режиме), свои голоса и ползунки пяти эффектов.
import type React from 'react';
import { useState } from 'react';
import type { Params, SoundNode } from '../graph';
import { useT } from '../i18n';
import { addVoice, hiddenVoices, KEEP_VOICE, removeVoice, restoreVoices, voiceName, voiceOf, voicesOf } from '../morpher';
import IconButton from './IconButton';

const MAX_NAME = 24;

/**
 * Кнопки голосов: готовые и свои. Используется в боковой панели и в простом режиме.
 * `editable` — ещё и «Сохранить как голос», убрать голос (×) и вернуть убранные готовые.
 */
export function VoicePicker({ params, onPick, editable }: { params: Params; onPick: (p: Params) => void; editable?: boolean }) {
  const t = useT();
  const active = voiceOf(params);
  const voices = voicesOf(params);
  const [naming, setNaming] = useState<string | null>(null);
  const [confirm, setConfirm] = useState<string | null>(null);
  const hidden = hiddenVoices(params).length;
  const save = () => {
    if (naming?.trim()) onPick(addVoice(params, naming.slice(0, MAX_NAME)));
    setNaming(null);
  };
  return (
    <>
      <div className="preset-row">
        {voices.map((v) => {
          const name = voiceName(v, t);
          if (confirm === v.id) {
            return (
              <span key={v.id} className="voice-confirm">
                {t('voice.removeAsk', { name })}
                <button
                  type="button"
                  className="danger"
                  onClick={() => {
                    setConfirm(null);
                    onPick(removeVoice(params, v.id));
                  }}
                >
                  {t('voice.removeYes')}
                </button>
                <button type="button" onClick={() => setConfirm(null)}>
                  {t('voice.cancel')}
                </button>
              </span>
            );
          }
          return (
            <span key={v.id} className={`voice-chip${editable && v.id !== KEEP_VOICE ? ' removable' : ''}`}>
              <button
                type="button"
                className={active === v.id ? 'primary' : undefined}
                aria-pressed={active === v.id}
                title={v.custom ? t('voice.customHint') : t(`voice.${v.id}.hint`)}
                onClick={() => onPick(v.params)}
              >
                {name}
              </button>
              {editable && v.id !== KEEP_VOICE && (
                <IconButton icon="close" label={t('voice.remove', { name })} onClick={() => setConfirm(v.id)} />
              )}
            </span>
          );
        })}
      </div>
      {editable && (
        <div className="voice-tools">
          {naming === null ? (
            <button type="button" disabled={active !== null} title={active !== null ? t('voice.saveSame') : undefined} onClick={() => setNaming('')}>
              {t('voice.save')}
            </button>
          ) : (
            <form
              className="voice-name"
              onSubmit={(e) => {
                e.preventDefault();
                save();
              }}
            >
              <input
                type="text"
                autoFocus
                maxLength={MAX_NAME}
                placeholder={t('voice.namePlaceholder')}
                value={naming}
                onChange={(e) => setNaming(e.target.value)}
                onKeyDown={(e) => e.key === 'Escape' && setNaming(null)}
              />
              <button type="submit" className="primary" disabled={!naming.trim()}>
                {t('voice.saveOk')}
              </button>
              <button type="button" onClick={() => setNaming(null)}>
                {t('voice.cancel')}
              </button>
            </form>
          )}
          {hidden > 0 && (
            <button type="button" className="link" onClick={() => onPick(restoreVoices())}>
              {t('voice.restore', { n: hidden })}
            </button>
          )}
        </div>
      )}
    </>
  );
}

function Amount({ label, value, onChange }: { label: string; value: number; onChange: (v: number) => void }) {
  const percent = Math.round(value * 100);
  return (
    <label className="field">
      <span>
        {label}: {percent} %
      </span>
      <input
        type="range"
        min={0}
        max={100}
        step={5}
        value={percent}
        style={{ '--fill': `${percent}%` } as React.CSSProperties}
        onChange={(e) => onChange(Number(e.target.value) / 100)}
      />
    </label>
  );
}

export default function MorpherParams({ node, onParams }: { node: SoundNode; onParams: (id: string, p: Params) => void }) {
  const t = useT();
  const p = node.data.params;
  const num = (k: string) => (typeof p[k] === 'number' ? (p[k] as number) : 0);
  const set = (patch: Params) => onParams(node.id, patch);
  const pitch = num('pitch');
  const formant = num('formant');
  const tone = num('tone');
  const semis = (v: number) => (v > 0 ? `+${v}` : v < 0 ? `−${-v}` : '0');
  return (
    <>
      <div className="field">
        <span>{t('param.voices')}</span>
        <VoicePicker params={p} onPick={set} editable />
      </div>
      <label className="field" title={t('param.toneHint')}>
        <span>
          {t('param.tone')}: {semis(tone)}
        </span>
        <input
          type="range"
          min={-12}
          max={12}
          step={1}
          value={tone}
          style={{ '--fill': `${((tone + 12) / 24) * 100}%` } as React.CSSProperties}
          onChange={(e) => set({ tone: Number(e.target.value) })}
        />
      </label>
      <label className="field" title={t('param.formantHint')}>
        <span>
          {t('param.formant')}: {semis(formant)}
        </span>
        <input
          type="range"
          min={-8}
          max={8}
          step={1}
          value={formant}
          style={{ '--fill': `${((formant + 8) / 16) * 100}%` } as React.CSSProperties}
          onChange={(e) => set({ formant: Number(e.target.value) })}
        />
      </label>
      <label className="field" title={t('param.pitchHint')}>
        <span>
          {t('param.pitch')}: {pitch > 0 ? `+${pitch}` : pitch < 0 ? `−${-pitch}` : '0'}
        </span>
        <input
          type="range"
          min={-12}
          max={12}
          step={1}
          value={pitch}
          style={{ '--fill': `${((pitch + 12) / 24) * 100}%` } as React.CSSProperties}
          onChange={(e) => set({ pitch: Number(e.target.value) })}
        />
      </label>
      <Amount label={t('param.robot')} value={num('robot')} onChange={(robot) => set({ robot })} />
      <Amount label={t('param.radio')} value={num('radio')} onChange={(radio) => set({ radio })} />
      <Amount label={t('param.wobble')} value={num('wobble')} onChange={(wobble) => set({ wobble })} />
      <Amount label={t('param.echo')} value={num('echo')} onChange={(echo) => set({ echo })} />
      <Amount label={t('param.growl')} value={num('growl')} onChange={(growl) => set({ growl })} />
      <Amount label={t('param.chorus')} value={num('chorus')} onChange={(chorus) => set({ chorus })} />
      <Amount label={t('param.hall')} value={num('hall')} onChange={(hall) => set({ hall })} />
      {(pitch !== 0 || num('wobble') > 0) && <p className="later">{t('param.pitchDelay')}</p>}
      {(formant !== 0 || tone !== 0) && <p className="later">{t('param.formantDelay')}</p>}
    </>
  );
}
