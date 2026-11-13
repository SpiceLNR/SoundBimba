// SPDX-License-Identifier: AGPL-3.0-or-later
// Шумоподавление: сила и индикатор «слышу голос / только фон».
import type React from 'react';
import { useAudio } from '../audio';
import type { Params, SoundNode } from '../graph';
import { useT } from '../i18n';
import { useFullId } from './chipContext';

/** Выше этой вероятности считаем, что сейчас звучит голос. */
const VOICE_LEVEL = 0.5;

export default function DenoiseParams({ node, onParams }: { node: SoundNode; onParams: (id: string, p: Params) => void }) {
  const t = useT();
  const audio = useAudio();
  const amount = typeof node.data.params.amount === 'number' ? node.data.params.amount : 1;
  const percent = Math.round(amount * 100);
  // Нет в файле (схемы до 2026-11-14) — 70 %, как в движке.
  const gate = typeof node.data.params.gate === 'number' ? node.data.params.gate : 0.7;
  const gatePercent = Math.round(gate * 100);
  const full = useFullId(node.id);
  const voice = audio.enabled ? (audio.voices?.[full] ?? 0) : 0;
  const speaking = voice >= VOICE_LEVEL;
  return (
    <>
      <label className="field">
        <span>
          {t('param.amount')}: {percent} %
        </span>
        <input
          type="range"
          min={0}
          max={100}
          step={5}
          value={percent}
          style={{ '--fill': `${percent}%` } as React.CSSProperties}
          onChange={(e) => onParams(node.id, { amount: Number(e.target.value) / 100 })}
        />
      </label>
      <label className="field" title={t('param.gateHint')}>
        <span>
          {t('param.gate')}: {gatePercent} %
        </span>
        <input
          type="range"
          min={0}
          max={100}
          step={5}
          value={gatePercent}
          style={{ '--fill': `${gatePercent}%` } as React.CSSProperties}
          onChange={(e) => onParams(node.id, { gate: Number(e.target.value) / 100 })}
        />
      </label>
      {audio.enabled && (
        <p className={`voice-state${speaking ? ' speaking' : ''}`} role="status">
          <span className="voice-dot" aria-hidden="true" />
          {t(speaking ? 'param.voiceHeard' : 'param.voiceNone')}
        </p>
      )}
      <p className="later">{t('param.denoiseHint')}</p>
    </>
  );
}
