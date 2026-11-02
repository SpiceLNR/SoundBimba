// SPDX-License-Identifier: AGPL-3.0-or-later
// Сетка кнопок-звуков: для кнопочной панели и простого режима.
import { api } from '../api';
import { isProblem, useAudio } from '../audio';
import type { ProjectNode } from '../graph';
import { useT } from '../i18n';
import ButtonDisc, { buttonLabel } from './ButtonDisc';

export default function ButtonGrid({ nodes, size = 100 }: { nodes: ProjectNode[]; size?: number }) {
  const t = useT();
  const audio = useAudio();
  const buttons = nodes.filter((n) => n.type === 'sound-button');
  if (buttons.length === 0) return <p className="panel-hint">{t('buttons.none')}</p>;
  return (
    <div className="button-grid" style={{ gridTemplateColumns: `repeat(auto-fill, minmax(${size + 28}px, 1fr))` }}>
      {buttons.map((n) => {
        const status = audio.status[n.id];
        const label = buttonLabel(n.params, t('node.sound-button.title'));
        return (
          <div key={n.id} className="button-tile">
            <ButtonDisc
              size={size}
              label={label}
              state={audio.enabled ? audio.buttons[n.id] : undefined}
              disabled={!audio.enabled || status !== 'ok'}
              onPress={() => void api.pressButton(n.id)}
              onSeek={(f) => void api.seekButton(n.id, f)}
            />
            <div className="button-tile-label" title={label}>
              {label}
            </div>
            {audio.enabled && isProblem(status) && <div className="button-tile-problem">{t(`status.${status}`)}</div>}
          </div>
        );
      })}
    </div>
  );
}
