// SPDX-License-Identifier: AGPL-3.0-or-later
// Узел на карте: цвет по типу, крупные порты, мало текста (настройки — в боковой панели).
// Пока звук включён: живой индикатор уровня, а если что-то не так — короткая подпись о проблеме.
import { Handle, Position, type NodeProps } from '@xyflow/react';
import { memo } from 'react';
import { api } from '../api';
import { isProblem, meterFraction, useAudio } from '../audio';
import ButtonDisc, { buttonLabel } from './ButtonDisc';
import { KINDS, summary, type SoundNode } from '../graph';
import { useT } from '../i18n';

function SoundNodeView({ id, data, selected }: NodeProps<SoundNode>) {
  const t = useT();
  const audio = useAudio();
  const info = KINDS[data.kind];
  const status = audio.status[id];
  const problem = audio.enabled && isProblem(status) ? t(`status.${status}`) : '';
  const isButton = data.kind === 'sound-button';
  const sub = problem || (isButton ? buttonLabel(data.params, '') || t('param.fileEmpty') : summary(data.kind, data.params, t));
  const level = audio.enabled ? (audio.levels[id] ?? 0) : 0;
  return (
    <div className={`sound-node cat-${info.category}${selected ? ' selected' : ''}${audio.enabled ? '' : ' sound-off'}`}>
      {info.input && <Handle type="target" position={Position.Left} />}
      <div className="sound-node-row">
        {isButton && (
          <ButtonDisc
            size={34}
            progress={audio.buttons[id]}
            disabled={!audio.enabled || status !== 'ok'}
            onPress={() => void api.pressButton(id)}
          />
        )}
        <div className="sound-node-text">
          <div className="sound-node-title">{t(`node.${data.kind}.title`)}</div>
          {sub && <div className={`sound-node-sub${problem ? ' problem' : ''}`}>{sub}</div>}
        </div>
      </div>
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
