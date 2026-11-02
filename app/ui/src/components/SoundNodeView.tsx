// SPDX-License-Identifier: AGPL-3.0-or-later
// Узел на карте: цвет по типу, крупные порты, мало текста (настройки — в боковой панели).
// Пока звук включён: живой индикатор уровня, а если что-то не так — короткая подпись о проблеме.
import { Handle, Position, type NodeProps } from '@xyflow/react';
import { memo } from 'react';
import { isProblem, meterFraction, useAudio } from '../audio';
import { KINDS, summary, type SoundNode } from '../graph';
import { useT } from '../i18n';

function SoundNodeView({ id, data, selected }: NodeProps<SoundNode>) {
  const t = useT();
  const audio = useAudio();
  const info = KINDS[data.kind];
  const status = audio.status[id];
  const problem = audio.enabled && isProblem(status) ? t(`status.${status}`) : '';
  const sub = problem || summary(data.kind, data.params, t);
  const level = audio.enabled ? (audio.levels[id] ?? 0) : 0;
  return (
    <div className={`sound-node cat-${info.category}${selected ? ' selected' : ''}${audio.enabled ? '' : ' sound-off'}`}>
      {info.input && <Handle type="target" position={Position.Left} />}
      <div className="sound-node-title">{t(`node.${data.kind}.title`)}</div>
      {sub && <div className={`sound-node-sub${problem ? ' problem' : ''}`}>{sub}</div>}
      {audio.enabled && (
        <div className="meter" aria-hidden="true">
          <div className={`meter-fill${level >= 0.99 ? ' clip' : ''}`} style={{ width: `${meterFraction(level) * 100}%` }} />
        </div>
      )}
      {info.output && <Handle type="source" position={Position.Right} />}
    </div>
  );
}

export default memo(SoundNodeView);
