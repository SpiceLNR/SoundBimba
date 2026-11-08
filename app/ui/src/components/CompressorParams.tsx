// SPDX-License-Identifier: AGPL-3.0-or-later
// Настройки компрессора в боковой панели: готовые наборы, порог, сила, усиление и (свёрнуто)
// атака с восстановлением. Под ними — сколько компрессор сжимает прямо сейчас.
import type React from 'react';
import { useAudio } from '../audio';
import { COMPRESSOR_PRESETS, presetOf, signed } from '../compressor';
import type { Params, SoundNode } from '../graph';
import { useT } from '../i18n';
import { useFullId } from './chipContext';

interface SliderProps {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  shown: string;
  hint?: string;
  onChange: (v: number) => void;
}

function Slider({ label, value, min, max, step, shown, hint, onChange }: SliderProps) {
  const fill = ((value - min) / (max - min)) * 100;
  return (
    <label className="field" title={hint}>
      <span>
        {label}: {shown}
      </span>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        style={{ '--fill': `${fill}%` } as React.CSSProperties}
        onChange={(e) => onChange(Number(e.target.value))}
      />
    </label>
  );
}

/** Сжатие до 24 дБ на шкале — больше на практике не бывает. */
const METER_DB = 24;

export default function CompressorParams({ node, onParams }: { node: SoundNode; onParams: (id: string, p: Params) => void }) {
  const t = useT();
  const audio = useAudio();
  const p = node.data.params;
  const num = (k: string, d: number) => (typeof p[k] === 'number' ? (p[k] as number) : d);
  const threshold = num('threshold', -24);
  const ratio = num('ratio', 3);
  const attack = num('attack', 5);
  const release = num('release', 120);
  const makeup = num('makeup', 12);
  const set = (patch: Params) => onParams(node.id, patch);
  const active = presetOf(p);
  const full = useFullId(node.id);
  const reduction = audio.enabled ? (audio.reductions?.[full] ?? 0) : 0;
  const db = t('unit.db');
  const ms = t('unit.ms');

  return (
    <>
      <div className="field">
        <span>{t('param.presets')}</span>
        <div className="preset-row">
          {COMPRESSOR_PRESETS.map((pr) => (
            <button
              key={pr.id}
              type="button"
              className={active === pr.id ? 'primary' : undefined}
              aria-pressed={active === pr.id}
              title={t(`preset.${pr.id}.hint`)}
              onClick={() => set(pr.params)}
            >
              {t(`preset.${pr.id}`)}
            </button>
          ))}
        </div>
      </div>
      <Slider
        label={t('param.threshold')}
        hint={t('param.thresholdHint')}
        value={threshold}
        min={-60}
        max={0}
        step={1}
        shown={`${signed(threshold)} ${db}`}
        onChange={(v) => set({ threshold: v })}
      />
      <Slider
        label={t('param.ratio')}
        hint={t('param.ratioHint')}
        value={ratio}
        min={1}
        max={20}
        step={0.5}
        shown={`${ratio}:1`}
        onChange={(v) => set({ ratio: v })}
      />
      <Slider
        label={t('param.makeup')}
        hint={t('param.makeupHint')}
        value={makeup}
        min={0}
        max={24}
        step={0.5}
        shown={`+${makeup} ${db}`}
        onChange={(v) => set({ makeup: v })}
      />
      <details className="more">
        <summary>{t('param.more')}</summary>
        <Slider
          label={t('param.attack')}
          hint={t('param.attackHint')}
          value={attack}
          min={0.5}
          max={100}
          step={0.5}
          shown={`${attack} ${ms}`}
          onChange={(v) => set({ attack: v })}
        />
        <Slider
          label={t('param.release')}
          hint={t('param.releaseHint')}
          value={release}
          min={10}
          max={1000}
          step={10}
          shown={`${release} ${ms}`}
          onChange={(v) => set({ release: v })}
        />
      </details>
      <div className="field">
        <span>{reduction >= 0.5 ? t('param.reduction', { db: reduction.toFixed(1) }) : t('param.reductionNone')}</span>
        <div className="reduction-meter" role="meter" aria-valuemin={0} aria-valuemax={METER_DB} aria-valuenow={reduction}>
          <div className="reduction-fill" style={{ width: `${Math.min(100, (reduction / METER_DB) * 100)}%` }} />
        </div>
      </div>
    </>
  );
}
