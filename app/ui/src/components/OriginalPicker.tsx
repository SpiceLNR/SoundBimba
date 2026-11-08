// SPDX-License-Identifier: AGPL-3.0-or-later
// «Оригинал звука» программы (этап 7): оставить как есть, направить на другое устройство или
// убрать на устройство, которое человек не слушает («Только через SoundBimba»). Меняется настройка
// Windows для этой программы — SoundBimba сама возвращает её (см. app/src-tauri/src/routes.rs).
import { useCallback, useEffect, useState } from 'react';
import { api, type AudioDevice } from '../api';
import { useAudio } from '../audio';
import type { Params, SoundNode } from '../graph';
import { useT } from '../i18n';
import Dropdown from './Dropdown';
import { useFullId } from './chipContext';

export type OriginalMode = 'normal' | 'device' | 'only';
const MODES: OriginalMode[] = ['normal', 'device', 'only'];

export function originalMode(params: Params): OriginalMode {
  return MODES.includes(params.original as OriginalMode) ? (params.original as OriginalMode) : 'normal';
}

export default function OriginalPicker({ node, onParams }: { node: SoundNode; onParams: (id: string, p: Params) => void }) {
  const t = useT();
  const audio = useAudio();
  const p = node.data.params;
  const exe = typeof p.exe === 'string' ? p.exe : '';
  const mode = originalMode(p);
  const device = typeof p.originalDevice === 'string' ? p.originalDevice : '';
  const [devices, setDevices] = useState<AudioDevice[] | null>(null);
  const load = useCallback(() => {
    api
      .listAudioDevices()
      .then(setDevices)
      .catch(() => setDevices([]));
  }, []);
  useEffect(load, [load]);

  // Виртуальный кабель в списке не нужен: оригинал попал бы прямо в виртуальный микрофон.
  const outputs = (devices ?? []).filter((d) => d.render && !d.cable);
  const chosen = outputs.find((d) => d.id === device);
  const full = useFullId(node.id);
  const state = audio.enabled ? audio.routes?.[full] : undefined;

  return (
    <div className="field original-picker">
      <span>{t('param.original')}</span>
      <div className="preset-row">
        {MODES.map((m) => (
          <button
            key={m}
            type="button"
            disabled={!exe}
            className={mode === m ? 'primary' : undefined}
            aria-pressed={mode === m}
            onClick={() => onParams(node.id, { original: m })}
          >
            {t(`original.${m}`)}
          </button>
        ))}
      </div>
      <p className="later">{t(exe ? `original.${mode}Hint` : 'original.needApp')}</p>
      {exe && mode !== 'normal' && (
        <>
          <Dropdown
            className="panel-dropdown"
            label={chosen ? chosen.name : t(device ? 'param.deviceUnknown' : 'original.pickDevice')}
            matchWidth
            onSelect={(id) => onParams(node.id, { originalDevice: id })}
            groups={[{ items: outputs.map((d) => ({ value: d.id, label: d.isDefault ? `${d.name} · ${t('original.isDefault')}` : d.name, selected: d.id === device })) }]}
          />
          {mode === 'only' && chosen?.isDefault && <p className="problem-text">{t('original.onlyDefaultWarning')}</p>}
          {device && !audio.enabled && <p className="later">{t('original.soundOff')}</p>}
          {device && state === 'applied' && <p className="later">{t('original.applied', { exe, device: chosen?.name ?? '' })}</p>}
          {device && state === 'waiting' && <p className="later">{t('original.waiting', { exe })}</p>}
          {device && state === 'error' && <p className="problem-text">{t('original.error')}</p>}
          <button type="button" className="link-button" onClick={load}>
            {t('param.refresh')}
          </button>
        </>
      )}
    </div>
  );
}
