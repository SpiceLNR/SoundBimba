// SPDX-License-Identifier: AGPL-3.0-or-later
// Узел на карте: цвет по типу, крупные порты, мало текста (настройки — в боковой панели).
// Пока звук включён: живой индикатор уровня, а если что-то не так — короткая подпись о проблеме.
import { Handle, Position, type NodeProps } from '@xyflow/react';
import { memo } from 'react';
import { api } from '../api';
import { isProblem, meterFraction, useAudio } from '../audio';
import ButtonDisc, { buttonLabel } from './ButtonDisc';
import { chipPins, KINDS, lookOf, summary, type SoundNode } from '../graph';
import { useT } from '../i18n';
import { useFullId } from './chipContext';

/** Микросхема: название и ножки — входы слева, выходы справа, у каждой своя точка. */
function ChipView({ data, selected }: Pick<NodeProps<SoundNode>, 'data' | 'selected'>) {
  const t = useT();
  const { ins, outs } = chipPins(data.params);
  const name = typeof data.params.name === 'string' ? data.params.name.trim() : '';
  const rows = Math.max(ins.length, outs.length);
  return (
    <div className={`sound-node chip-node cat-chip${selected ? ' selected' : ''}`} title={t('chip.openHint')}>
      <div className="sound-node-title">{name || t('node.chip.title')}</div>
      {rows === 0 && <div className="sound-node-sub">{t('chip.noPins')}</div>}
      <div className="chip-pins">
        {Array.from({ length: rows }, (_, i) => (
          <div key={i} className="chip-pin-row">
            <span className="chip-pin in">
              {ins[i] && (
                <>
                  <Handle type="target" position={Position.Left} id={ins[i].id} />
                  {ins[i].name || t('chip.pinIn', { n: i + 1 })}
                </>
              )}
            </span>
            <span className="chip-pin out">
              {outs[i] && (
                <>
                  {outs[i].name || t('chip.pinOut', { n: i + 1 })}
                  <Handle type="source" position={Position.Right} id={outs[i].id} />
                </>
              )}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}

function SoundNodeView({ id: localId, data, selected }: NodeProps<SoundNode>) {
  const t = useT();
  const audio = useAudio();
  // Внутри микросхемы движок знает узел по полному id.
  const id = useFullId(localId);
  if (data.kind === 'chip') return <ChipView data={data} selected={selected} />;
  const info = KINDS[data.kind];
  const status = audio.status[id];
  const problem = audio.enabled && isProblem(status) ? t(`status.${status}`) : '';
  const isButton = data.kind === 'sound-button';
  const sub = problem || (isButton ? buttonLabel(data.params, '') || t('param.fileEmpty') : summary(data.kind, data.params, t));
  const level = audio.enabled ? (audio.levels[id] ?? 0) : 0;
  return (
    <div className={`sound-node cat-${lookOf(data.kind)}${selected ? ' selected' : ''}${audio.enabled ? '' : ' sound-off'}`}>
      {info.input && <Handle type="target" position={Position.Left} />}
      <div className="sound-node-row">
        {isButton && (
          <ButtonDisc
            size={76}
            state={audio.enabled ? audio.buttons[id] : undefined}
            disabled={!audio.enabled || status !== 'ok'}
            onPress={() => void api.pressButton(id)}
            onSeek={(f) => void api.seekButton(id, f)}
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
