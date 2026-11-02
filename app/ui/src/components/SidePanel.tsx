// SPDX-License-Identifier: AGPL-3.0-or-later
// Боковая панель: настройки выбранного узла или провода.
import { open } from '@tauri-apps/plugin-dialog';
import type { Edge } from '@xyflow/react';
import { useCallback, useEffect, useState } from 'react';
import type React from 'react';
import { api, type AudioApp, type AudioDevice } from '../api';
import { isProblem, useAudio } from '../audio';
import { KINDS, type NodeKind, type Params, type SoundNode } from '../graph';
import { useT } from '../i18n';
import CompressorParams from './CompressorParams';
import ButtonDisc from './ButtonDisc';
import Dropdown from './Dropdown';

interface Props {
  node?: SoundNode;
  edge?: Edge;
  edgeLabel?: string;
  onParams: (id: string, patch: Params) => void;
  onDeleteNode: (id: string) => void;
  onDeleteEdge: (id: string) => void;
}

/** Выбор устройства: микрофон, наушники или виртуальный кабель. */
function DevicePicker({ kind, value, onChange }: { kind: NodeKind; value: string; onChange: (v: string) => void }) {
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

/** Программа для узла «Звук приложения»: ввести имя exe или выбрать из тех, что сейчас со звуком. */
function AppPicker({ value, onChange }: { value: string; onChange: (exe: string) => void }) {
  const t = useT();
  const [apps, setApps] = useState<AudioApp[]>([]);
  const load = useCallback(() => {
    api
      .listAudioApps()
      .then(setApps)
      .catch(() => setApps([]));
  }, []);
  useEffect(load, [load]);
  return (
    <div className="field">
      <span>{t('param.exe')}</span>
      <input
        type="text"
        value={value}
        placeholder={t('param.exePlaceholder')}
        spellCheck={false}
        onChange={(e) => onChange(e.target.value.trim())}
      />
      <Dropdown
        className="panel-dropdown"
        label={t('param.pickApp')}
        matchWidth
        onSelect={onChange}
        groups={[
          {
            items: apps.length
              ? apps.map((a) => ({
                  value: a.exe,
                  label: a.active ? `${a.exe} · ${t('param.appPlaying')}` : a.exe,
                  selected: a.exe.toLowerCase() === value.toLowerCase(),
                }))
              : [{ value, label: t('param.appsEmpty') }],
          },
        ]}
      />
      <button type="button" className="link-button" onClick={load}>
        {t('param.refresh')}
      </button>
    </div>
  );
}

/** Кнопка-звук: файл, название, проверка. */
function ButtonParams({ node, onParams }: { node: SoundNode; onParams: Props['onParams'] }) {
  const t = useT();
  const audio = useAudio();
  const { params } = node.data;
  const file = typeof params.file === 'string' ? params.file : '';
  const pick = async () => {
    const path = await open({
      multiple: false,
      directory: false,
      filters: [{ name: t('dialog.audioFilter'), extensions: ['wav', 'mp3', 'ogg', 'flac'] }],
    });
    if (typeof path === 'string') onParams(node.id, { file: path });
  };
  return (
    <>
      <div className="field">
        <span>{t('param.file')}</span>
        <div className="value file-value" title={file}>
          {file ? (file.split(/[\\/]/).pop() ?? file) : t('param.fileEmpty')}
        </div>
        <button type="button" onClick={() => void pick()}>
          {t('param.pickFile')}
        </button>
        <p className="later">{t('param.fileFormats')}</p>
        <p className="later">{t('button.seekHint')}</p>
      </div>
      <label className="field">
        <span>{t('param.label')}</span>
        <input
          type="text"
          value={typeof params.label === 'string' ? params.label : ''}
          placeholder={t('param.labelPlaceholder')}
          onChange={(e) => onParams(node.id, { label: e.target.value })}
        />
      </label>
      <div className="field">
        <span>{t('param.tryButton')}</span>
        <ButtonDisc
          size={120}
          state={audio.enabled ? audio.buttons[node.id] : undefined}
          disabled={!audio.enabled || audio.status[node.id] !== 'ok'}
          onPress={() => void api.pressButton(node.id)}
          onSeek={(f) => void api.seekButton(node.id, f)}
        />
      </div>
    </>
  );
}

function NodeParams({ node, onParams }: { node: SoundNode; onParams: Props['onParams'] }) {
  const t = useT();
  const { kind, params } = node.data;
  const set = (patch: Params) => onParams(node.id, patch);
  switch (kind) {
    case 'app-source':
      return <AppPicker value={typeof params.exe === 'string' ? params.exe : ''} onChange={(exe) => set({ exe })} />;
    case 'volume': {
      const percent = Math.round(Number(params.gain ?? 1) * 100);
      return (
        <>
          <label className="field">
            <span>
              {t('param.gain')}: {percent} %
            </span>
            <input
              type="range"
              min={0}
              max={200}
              step={5}
              value={percent}
              style={{ '--fill': `${percent / 2}%` } as React.CSSProperties}
              onChange={(e) => set({ gain: Number(e.target.value) / 100 })}
            />
          </label>
          <label className="check">
            <input type="checkbox" checked={params.mute === true} onChange={(e) => set({ mute: e.target.checked })} />
            <span>{t('param.mute')}</span>
          </label>
        </>
      );
    }
    case 'sound-button':
      return <ButtonParams node={node} onParams={onParams} />;
    case 'compressor':
      return <CompressorParams node={node} onParams={onParams} />;
    case 'mic-input':
    case 'speaker-out':
    case 'virtual-out': {
      const fallback = kind === 'virtual-out' ? 'virtual-1' : 'default';
      const value = typeof params.device === 'string' && params.device ? params.device : fallback;
      return <DevicePicker kind={kind} value={value} onChange={(device) => set({ device })} />;
    }
  }
}

export default function SidePanel({ node, edge, edgeLabel, onParams, onDeleteNode, onDeleteEdge }: Props) {
  const t = useT();
  const audio = useAudio();
  if (node) {
    const status = audio.status[node.id];
    return (
      <aside className="side-panel">
        <h2 className={`panel-title cat-${KINDS[node.data.kind].category}`}>{t(`node.${node.data.kind}.title`)}</h2>
        <p className="panel-desc">{t(`node.${node.data.kind}.desc`)}</p>
        {audio.enabled && isProblem(status) && <p className="problem-text">{t(`status.${status}`)}</p>}
        <NodeParams key={node.id} node={node} onParams={onParams} />
        <button type="button" className="danger" onClick={() => onDeleteNode(node.id)}>
          {t('panel.deleteNode')}
        </button>
      </aside>
    );
  }
  if (edge) {
    return (
      <aside className="side-panel">
        <h2 className="panel-title">{t('panel.edge')}</h2>
        <p className="panel-desc">{edgeLabel}</p>
        <button type="button" className="danger" onClick={() => onDeleteEdge(edge.id)}>
          {t('panel.deleteEdge')}
        </button>
      </aside>
    );
  }
  return (
    <aside className="side-panel">
      <p className="panel-desc">{t('panel.empty')}</p>
      <p className="panel-hint">{t('panel.hint')}</p>
      {!audio.enabled && <p className="panel-hint">{t('panel.soundOffHint')}</p>}
    </aside>
  );
}
