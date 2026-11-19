// SPDX-License-Identifier: AGPL-3.0-or-later
// «Диктофон» в боковой панели: запись, прослушивание мимо схемы (на выбранное устройство) и
// сохранение последней записи в файл.
import { save } from '@tauri-apps/plugin-dialog';
import { useState } from 'react';
import type React from 'react';
import { api, isCmdError } from '../api';
import { useAudio } from '../audio';
import type { Params, SoundNode } from '../graph';
import { DICTS, useT, type Translate } from '../i18n';
import { formatTime } from './ButtonDisc';
import { useFullId } from './chipContext';
import DevicePicker from './DevicePicker';
import RecordButton, { recorderLabel } from './RecordButton';

/** Для имени файла: «2026-11-19 15-30». */
function stamp(d = new Date()): string {
  const p = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}-${p(d.getMinutes())}`;
}

function reasonText(e: unknown, t: Translate): string {
  const code = isCmdError(e) ? e.code : 'other';
  return t(DICTS.en[`reason.${code}`] ? `reason.${code}` : 'reason.other');
}

export default function RecorderParams({ node, onParams }: { node: SoundNode; onParams: (id: string, patch: Params) => void }) {
  const t = useT();
  const audio = useAudio();
  const full = useFullId(node.id);
  const info = audio.recorders?.[full];
  const player = audio.player && audio.player.id === full ? audio.player : null;
  const [message, setMessage] = useState<{ text: string; error: boolean } | null>(null);
  const device = typeof node.data.params.playDevice === 'string' && node.data.params.playDevice ? node.data.params.playDevice : 'default';
  const recording = info?.recording === true;
  const hasTake = info?.hasTake === true && !recording;

  const fail = (e: unknown) => setMessage({ text: t('recorder.failed', { reason: reasonText(e, t) }), error: true });
  const toggle = () => {
    setMessage(null);
    api.recorderToggle(full).catch(fail);
  };
  const listen = () => {
    setMessage(null);
    (player ? api.recorderStopListening() : api.recorderPlay(full, device)).catch(fail);
  };
  const saveFile = async () => {
    setMessage(null);
    const path = await save({
      defaultPath: `${t('recorder.fileName', { date: stamp() })}.wav`,
      filters: [{ name: t('dialog.wavFilter'), extensions: ['wav'] }],
    });
    if (!path) return;
    try {
      await api.recorderSave(full, path);
      setMessage({ text: t('recorder.savedOk', { name: path.split(/[\\/]/).pop() ?? path }), error: false });
    } catch (e) {
      fail(e);
    }
  };

  return (
    <>
      <div className="field">
        <span>{t('recorder.record')}</span>
        <div className="recorder-row">
          <RecordButton size={72} recording={recording} disabled={!audio.running} onToggle={toggle} />
          <span className={`recorder-status${recording ? ' recording' : ''}`}>{recorderLabel(info, t)}</span>
        </div>
        <p className="panel-hint">{t(audio.running ? 'recorder.hint' : 'recorder.soundOff')}</p>
        {info?.lost && <p className="problem-text">{t('recorder.lost')}</p>}
        {info?.error && <p className="problem-text">{t('recorder.failed', { reason: t(`reason.${info.error}`) })}</p>}
      </div>
      <div className="field">
        <span>{t('recorder.listen')}</span>
        <div className="recorder-row">
          <button type="button" className={player ? 'primary' : undefined} disabled={!hasTake} onClick={listen}>
            {t(player ? 'recorder.stopListening' : 'recorder.play')}
          </button>
          {player && (
            <span className="recorder-time">
              {formatTime(player.pos)} / {formatTime(player.len)}
            </span>
          )}
        </div>
        {player && player.len > 0 && (
          <input
            type="range"
            min={0}
            max={1000}
            value={Math.round((player.pos / player.len) * 1000)}
            aria-label={t('recorder.seek')}
            style={{ '--fill': `${(player.pos / player.len) * 100}%` } as React.CSSProperties}
            onChange={(e) => void api.recorderSeek(Number(e.target.value) / 1000)}
          />
        )}
      </div>
      <DevicePicker kind="speaker-out" label={t('recorder.listenOn')} value={device} onChange={(playDevice) => onParams(node.id, { playDevice })} />
      <p className="panel-hint">{t('recorder.listenHint')}</p>
      <button type="button" disabled={!hasTake} onClick={() => void saveFile()}>
        {t('recorder.save')}
      </button>
      {message && <p className={message.error ? 'problem-text' : 'panel-hint'}>{message.text}</p>}
      <p className="panel-hint">{t('recorder.keepHint')}</p>
    </>
  );
}
