// SPDX-License-Identifier: AGPL-3.0-or-later
// «Подстроить под мой голос»: 5 секунд с микрофона — высота голоса; готовые голоса морфера
// подстраиваются под неё (см. morpher.ts, `adapted`). Запись нигде не сохраняется.
import { useEffect, useState } from 'react';
import { api, isCmdError, type VoiceMeasure } from '../api';
import { useT } from '../i18n';
import DevicePicker from './DevicePicker';

const SECONDS = 5;

type Stage = { kind: 'ready' } | { kind: 'listening'; left: number } | { kind: 'done'; v: VoiceMeasure } | { kind: 'error'; code: string };

/** Словами: низкий, средний или высокий голос. */
function range(hz: number): 'low' | 'middle' | 'high' {
  return hz < 140 ? 'low' : hz < 190 ? 'middle' : 'high';
}

interface Props {
  /** Микрофон сразу в списке: тот, что идёт в этот морфер на карте, или «default». */
  device: string;
  onSave: (f0: number) => void;
  onClose: () => void;
}

export default function VoiceCalibration({ device: initial, onSave, onClose }: Props) {
  const t = useT();
  const [stage, setStage] = useState<Stage>({ kind: 'ready' });
  const [device, setDevice] = useState(initial);

  useEffect(() => {
    if (stage.kind !== 'listening' || stage.left <= 0) return;
    const id = window.setTimeout(() => setStage((s) => (s.kind === 'listening' ? { kind: 'listening', left: s.left - 1 } : s)), 1000);
    return () => window.clearTimeout(id);
  }, [stage]);

  const start = () => {
    setStage({ kind: 'listening', left: SECONDS });
    api
      .measureVoice(device)
      .then((v) => setStage({ kind: 'done', v }))
      .catch((e) => setStage({ kind: 'error', code: isCmdError(e) && (e.code === 'no-mic' || e.code === 'no-voice') ? e.code : 'io' }));
  };

  return (
    <div className="dialog-backdrop">
      <div className="dialog dialog-narrow" role="dialog" aria-modal="true" aria-labelledby="voice-cal-title">
        <h2 id="voice-cal-title" className="panel-title">
          {t('voiceCal.title')}
        </h2>
        {(stage.kind === 'ready' || stage.kind === 'listening') && (
          <>
            <p className="panel-desc">{t('voiceCal.text')}</p>
            {/* Фраза почти из одних звонких звуков: по ним высота голоса слышна лучше всего. */}
            <p className="voice-cal-phrase">{t('voiceCal.phrase')}</p>
            {stage.kind === 'ready' && <p className="panel-desc">{t('voiceCal.why')}</p>}
          </>
        )}
        {stage.kind === 'listening' && (
          <p className="panel-desc voice-cal-listening" aria-live="polite">
            {t('voiceCal.listening', { n: Math.max(stage.left, 0) })}
          </p>
        )}
        {stage.kind === 'done' && (
          <p className="panel-desc" aria-live="polite">
            {t('voiceCal.result', { hz: Math.round(stage.v.medianHz), range: t(`voiceCal.range.${range(stage.v.medianHz)}`) })}
          </p>
        )}
        {stage.kind !== 'listening' && <DevicePicker kind="mic-input" value={device} onChange={setDevice} />}
        {stage.kind === 'error' && (
          <p className="panel-desc problem" aria-live="polite">
            {t(`voiceCal.error.${stage.code}`)}
          </p>
        )}
        <div className="dialog-actions">
          <span className="toolbar-gap" />
          <button type="button" onClick={onClose}>
            {t('voiceCal.cancel')}
          </button>
          {stage.kind === 'done' ? (
            <>
              <button type="button" onClick={start}>
                {t('voiceCal.again')}
              </button>
              <button type="button" className="primary" autoFocus onClick={() => onSave(stage.v.medianHz)}>
                {t('voiceCal.save')}
              </button>
            </>
          ) : (
            <button type="button" className="primary" autoFocus disabled={stage.kind === 'listening'} onClick={start}>
              {stage.kind === 'error' ? t('voiceCal.again') : t('voiceCal.start')}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
