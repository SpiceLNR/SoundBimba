// SPDX-License-Identifier: AGPL-3.0-or-later
// Морфер голоса: выбор готового голоса (он же — в простом режиме) и ползунки пяти эффектов.
import type React from 'react';
import type { Params, SoundNode } from '../graph';
import { useT } from '../i18n';
import { VOICES, voiceOf } from '../morpher';

/** Кнопки готовых голосов. Используется в боковой панели и в простом режиме. */
export function VoicePicker({ params, onPick }: { params: Params; onPick: (p: Params) => void }) {
  const t = useT();
  const active = voiceOf(params);
  return (
    <div className="preset-row">
      {VOICES.map((v) => (
        <button
          key={v.id}
          type="button"
          className={active === v.id ? 'primary' : undefined}
          aria-pressed={active === v.id}
          title={t(`voice.${v.id}.hint`)}
          onClick={() => onPick(v.params)}
        >
          {t(`voice.${v.id}`)}
        </button>
      ))}
    </div>
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
  return (
    <>
      <div className="field">
        <span>{t('param.voices')}</span>
        <VoicePicker params={p} onPick={set} />
      </div>
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
      {(pitch !== 0 || num('wobble') > 0) && <p className="later">{t('param.pitchDelay')}</p>}
    </>
  );
}
