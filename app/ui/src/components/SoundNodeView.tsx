// SPDX-License-Identifier: AGPL-3.0-or-later
// Узел на карте: цвет по типу, крупные порты, мало текста (настройки — в боковой панели).
import { Handle, Position, type NodeProps } from '@xyflow/react';
import { memo } from 'react';
import { KINDS, summary, type SoundNode } from '../graph';
import { useT } from '../i18n';

function SoundNodeView({ data, selected }: NodeProps<SoundNode>) {
  const t = useT();
  const info = KINDS[data.kind];
  const sub = summary(data.kind, data.params, t);
  return (
    <div className={`sound-node cat-${info.category}${selected ? ' selected' : ''}`}>
      {info.input && <Handle type="target" position={Position.Left} />}
      <div className="sound-node-title">{t(`node.${data.kind}.title`)}</div>
      {sub && <div className="sound-node-sub">{sub}</div>}
      {info.output && <Handle type="source" position={Position.Right} />}
    </div>
  );
}

export default memo(SoundNodeView);
