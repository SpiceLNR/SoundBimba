// SPDX-License-Identifier: AGPL-3.0-or-later
// Боковая панель: настройки выбранного узла или провода.
import type { Edge } from '@xyflow/react';
import type React from 'react';
import { KINDS, type Params, type SoundNode } from '../graph';
import { useT } from '../i18n';

interface Props {
  node?: SoundNode;
  edge?: Edge;
  edgeLabel?: string;
  onParams: (id: string, patch: Params) => void;
  onDeleteNode: (id: string) => void;
  onDeleteEdge: (id: string) => void;
}

function NodeParams({ node, onParams }: { node: SoundNode; onParams: Props['onParams'] }) {
  const t = useT();
  const { kind, params } = node.data;
  const set = (patch: Params) => onParams(node.id, patch);
  switch (kind) {
    case 'app-source':
      return (
        <label className="field">
          <span>{t('param.exe')}</span>
          <input
            type="text"
            value={typeof params.exe === 'string' ? params.exe : ''}
            placeholder={t('param.exePlaceholder')}
            spellCheck={false}
            onChange={(e) => set({ exe: e.target.value.trim() })}
          />
        </label>
      );
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
      return (
        <div className="field">
          <span>{t('param.file')}</span>
          <div className="value">{typeof params.file === 'string' && params.file ? params.file : t('param.fileEmpty')}</div>
          <p className="later">{t('param.fileLater')}</p>
        </div>
      );
    case 'mic-input':
    case 'speaker-out':
    case 'virtual-out':
      return (
        <div className="field">
          <span>{t('param.device')}</span>
          <div className="value">{t('param.deviceDefault')}</div>
          <p className="later">{t('param.deviceLater')}</p>
        </div>
      );
  }
}

export default function SidePanel({ node, edge, edgeLabel, onParams, onDeleteNode, onDeleteEdge }: Props) {
  const t = useT();
  if (node) {
    return (
      <aside className="side-panel">
        <h2 className={`panel-title cat-${KINDS[node.data.kind].category}`}>{t(`node.${node.data.kind}.title`)}</h2>
        <p className="panel-desc">{t(`node.${node.data.kind}.desc`)}</p>
        <NodeParams node={node} onParams={onParams} />
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
    </aside>
  );
}
