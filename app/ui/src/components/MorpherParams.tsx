// SPDX-License-Identifier: AGPL-3.0-or-later
// Морфер голоса: выбор голоса (он же — в простом режиме), свои голоса и ползунки пяти эффектов.
import type React from 'react';
import { useReactFlow } from '@xyflow/react';
import { useState } from 'react';
import type { Params, SoundNode } from '../graph';
import { useT } from '../i18n';
import { addVoice, hiddenVoices, KEEP_VOICE, removeVoice, restoreVoices, voiceName, voiceOf, voicesOf } from '../morpher';
import IconButton from './IconButton';
import VoiceCalibration from './VoiceCalibration';
import { useVoiceProfile } from './voiceProfile';

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
              <span key={v.id} className="voice-confirm" role="group" aria-label={t('voice.removeAsk', { name })}>
                <span className="voice-confirm-text">{t('voice.removeAsk', { name })}</span>
                <span className="voice-confirm-buttons">
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

/** Микрофон, который идёт в этот узел (через обработку на карте), или «default». */
function micFeeding(nodeId: string, nodes: SoundNode[], edges: { source: string; target: string }[]): string {
  const seen = new Set<string>();
  const queue = [nodeId];
  while (queue.length) {
    const id = queue.shift()!;
    if (seen.has(id)) continue;
    seen.add(id);
    const n = nodes.find((x) => x.id === id);
    if (n?.data.kind === 'mic-input') {
      const d = n.data.params.device;
      return typeof d === 'string' && d ? d : 'default';
    }
    for (const e of edges) if (e.target === id) queue.push(e.source);
  }
  return 'default';
}

/** Подстроены ли голоса под голос человека; кнопка подстройки. */
function VoiceProfileLine({ nodeId }: { nodeId: string }) {
  const t = useT();
  const { f0, onChange } = useVoiceProfile();
  const [open, setOpen] = useState(false);
  const rf = useReactFlow<SoundNode>();
  return (
    <div className="voice-profile">
      <span>{f0 === null ? t('voiceCal.none') : t('voiceCal.set', { hz: Math.round(f0) })}</span>
      <button type="button" className="link" onClick={() => setOpen(true)}>
        {f0 === null ? t('voiceCal.open') : t('voiceCal.redo')}
      </button>
      {f0 !== null && (
        <button type="button" className="link" onClick={() => onChange(null)}>
          {t('voiceCal.reset')}
        </button>
      )}
      {open && (
        <VoiceCalibration
          device={micFeeding(nodeId, rf.getNodes(), rf.getEdges())}
          onClose={() => setOpen(false)}
          onSave={(hz) => {
            onChange(hz);
            setOpen(false);
          }}
        />
      )}
    </div>
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
        <VoiceProfileLine nodeId={node.id} />
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
      <Amount label={t('param.lowcut')} value={num('lowcut')} onChange={(lowcut) => set({ lowcut })} />
      <Amount label={t('param.bright')} value={num('bright')} onChange={(bright) => set({ bright })} />
      {(pitch !== 0 || num('wobble') > 0) && <p className="later">{t('param.pitchDelay')}</p>}
      {(formant !== 0 || tone !== 0) && <p className="later">{t('param.formantDelay')}</p>}
    </>
  );
}
