// SPDX-License-Identifier: AGPL-3.0-or-later
// Простой режим: без карты. Кнопки-звуки, громкость с подписью «откуда → куда», место под морфер.
import type React from 'react';
import { volumeRoutes, type Params, type Project } from '../graph';
import { useT } from '../i18n';
import ButtonGrid from './ButtonGrid';

interface Props {
  project: Project;
  onParams: (id: string, patch: Params) => void;
}

export default function SimpleView({ project, onParams }: Props) {
  const t = useT();
  const routes = volumeRoutes(project, t);
  return (
    <div className="simple-view">
      <section className="simple-section">
        <h2 className="panel-title">{t('simple.buttons')}</h2>
        <ButtonGrid nodes={project.nodes} size={124} />
      </section>

      <section className="simple-section">
        <h2 className="panel-title">{t('simple.volume')}</h2>
        {routes.length === 0 && <p className="panel-hint">{t('simple.noVolume')}</p>}
        {routes.map((r) => {
          const node = project.nodes.find((n) => n.id === r.id)!;
          const percent = Math.round(Number(node.params.gain ?? 1) * 100);
          return (
            <div key={r.id} className="simple-volume">
              <div className="simple-route">
                <span>{r.from || t('simple.nothing')}</span>
                <span className="simple-arrow">→</span>
                <span>{r.to || t('simple.nothing')}</span>
              </div>
              <div className="simple-controls">
                <input
                  type="range"
                  min={0}
                  max={200}
                  step={5}
                  value={percent}
                  aria-label={`${r.from} → ${r.to}`}
                  style={{ '--fill': `${percent / 2}%` } as React.CSSProperties}
                  onChange={(e) => onParams(r.id, { gain: Number(e.target.value) / 100 })}
                />
                <span className="simple-percent">{percent} %</span>
                <label className="check">
                  <input
                    type="checkbox"
                    checked={node.params.mute === true}
                    onChange={(e) => onParams(r.id, { mute: e.target.checked })}
                  />
                  <span>{t('param.mute')}</span>
                </label>
              </div>
            </div>
          );
        })}
      </section>

      <section className="simple-section simple-later">
        <h2 className="panel-title">{t('simple.morpher')}</h2>
        <p className="panel-hint">{t('simple.morpherLater')}</p>
      </section>
    </div>
  );
}
