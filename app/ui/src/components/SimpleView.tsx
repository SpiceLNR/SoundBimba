// SPDX-License-Identifier: AGPL-3.0-or-later
// Простой режим: без карты. Кнопки-звуки, громкость с подписью «откуда → куда», голоса морфера.
// Элементы каждого раздела можно переставлять (ручка слева сверху); порядок хранится в узлах и
// попадает в проект и шаблоны. Длинные перечисления «откуда / куда» сворачиваются.
import { useState, type CSSProperties } from 'react';
import { simpleSorted, volumeRoutes, type Params, type Project, type VolumeRoute } from '../graph';
import { useT } from '../i18n';
import ButtonGrid from './ButtonGrid';
import { VoicePicker } from './MorpherParams';
import { useReorder, type Reorder } from './useReorder';

/** Сколько названий показывать до «и ещё N». */
const SHOWN_NAMES = 3;

interface Props {
  project: Project;
  onParams: (id: string, patch: Params) => void;
}

function Names({ names }: { names: string[] }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  if (!names.length) return <span>{t('simple.nothing')}</span>;
  const long = names.length > SHOWN_NAMES;
  const shown = open || !long ? names : names.slice(0, SHOWN_NAMES);
  return (
    <span className="simple-names">
      <span>{shown.join(', ')}</span>
      {long && (
        <button type="button" className="link-button simple-more" aria-expanded={open} onClick={() => setOpen(!open)}>
          {open ? t('simple.less') : t('simple.more', { count: names.length - SHOWN_NAMES })}
        </button>
      )}
    </span>
  );
}

function Route({ r, reorder, label }: { r: VolumeRoute; reorder: Reorder; label: string }) {
  const t = useT();
  return (
    <div className="simple-route">
      <button type="button" className="reorder-handle" aria-label={t('simple.move', { name: label })} title={t('simple.moveHint')} {...reorder.handle(r.id)} />
      <Names names={r.fromNames} />
      <span className="simple-arrow">→</span>
      <Names names={r.toNames} />
    </div>
  );
}

export default function SimpleView({ project, onParams }: Props) {
  const t = useT();
  const routes = volumeRoutes(project, t);
  const morphers = volumeRoutes(project, t, 'voice-morpher');
  const renumber = (order: Record<string, number>) => {
    for (const [id, n] of Object.entries(order)) onParams(id, { simpleOrder: n });
  };
  const buttonIds = simpleSorted(project.nodes.filter((n) => n.type === 'sound-button')).map((n) => n.id);
  const buttons = useReorder('buttons', buttonIds, renumber);
  const volumes = useReorder('volumes', routes.map((r) => r.id), renumber);
  const voices = useReorder('voices', morphers.map((r) => r.id), renumber);
  return (
    <div className="simple-view">
      <section className="simple-section">
        <h2 className="panel-title">{t('simple.buttons')}</h2>
        <ButtonGrid nodes={project.nodes} size={124} reorder={buttons} />
      </section>

      <section className="simple-section">
        <h2 className="panel-title">{t('simple.volume')}</h2>
        {routes.length === 0 && <p className="panel-hint">{t('simple.noVolume')}</p>}
        {routes.map((r) => {
          const node = project.nodes.find((n) => n.id === r.id)!;
          const percent = Math.round(Number(node.params.gain ?? 1) * 100);
          const drag = volumes.item(r.id);
          return (
            <div key={r.id} {...drag} className={`simple-volume${drag.className}`}>
              <Route r={r} reorder={volumes} label={`${r.from} → ${r.to}`} />
              <div className="simple-controls">
                <input
                  type="range"
                  min={0}
                  max={200}
                  step={5}
                  value={percent}
                  aria-label={`${r.from} → ${r.to}`}
                  style={{ '--fill': `${percent / 2}%` } as CSSProperties}
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

      <section className="simple-section">
        <h2 className="panel-title">{t('simple.morpher')}</h2>
        {morphers.length === 0 && <p className="panel-hint">{t('simple.morpherNone')}</p>}
        {morphers.map((r) => {
          const drag = voices.item(r.id);
          return (
            <div key={r.id} {...drag} className={`simple-volume${drag.className}`}>
              <Route r={r} reorder={voices} label={`${r.from} → ${r.to}`} />
              <VoicePicker params={project.nodes.find((n) => n.id === r.id)!.params} onPick={(p) => onParams(r.id, p)} />
            </div>
          );
        })}
      </section>
    </div>
  );
}
