// SPDX-License-Identifier: AGPL-3.0-or-later
// Выбор устройства: микрофон, наушники или виртуальный кабель (боковая панель, подстройка голоса).
import { useCallback, useEffect, useState } from 'react';
import { api, type AudioDevice } from '../api';
import type { NodeKind } from '../graph';
import { useT } from '../i18n';
import Dropdown from './Dropdown';

/** Выбор устройства: микрофон, наушники или виртуальный кабель. */
export default function DevicePicker({ kind, value, onChange }: { kind: NodeKind; value: string; onChange: (v: string) => void }) {
  const t = useT();
  const [devices, setDevices] = useState<AudioDevice[] | null>(null);
  const load = useCallback(() => {
    api
      .listAudioDevices()
      .then(setDevices)
      .catch(() => setDevices([]));
  }, []);
  useEffect(load, [load]);

  const render = kind !== 'mic-input';
  const list = (devices ?? []).filter((d) => d.render === render && (kind !== 'virtual-out' || d.cable));
  const byDefault = list.find((d) => d.isDefault);
  const special =
    kind === 'virtual-out'
      ? { value: 'virtual-1', label: t('param.deviceAuto') }
      : { value: 'default', label: byDefault ? t('param.deviceDefaultNamed', { name: byDefault.name }) : t('param.deviceDefault') };
  const items = [special, ...list.map((d) => ({ value: d.id, label: d.name }))].map((it) => ({
    ...it,
    selected: it.value === value,
  }));
  const current = items.find((it) => it.selected)?.label ?? (devices ? t('param.deviceUnknown') : '…');

  return (
    <div className="field">
      <span>{t('param.device')}</span>
      <Dropdown className="panel-dropdown" label={current} matchWidth groups={[{ items }]} onSelect={onChange} />
      {kind === 'virtual-out' && devices && list.length === 0 && <p className="problem-text">{t('param.noCableHint')}</p>}
      <button type="button" className="link-button" onClick={load}>
        {t('param.refresh')}
      </button>
    </div>
  );
}

