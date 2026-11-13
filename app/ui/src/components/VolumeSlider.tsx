// SPDX-License-Identifier: AGPL-3.0-or-later
// Ползунок громкости как на пульте: в децибелах, крайнее левое — выключено, отметка 0 дБ.
// Двойной щелчок — обратно на 0 дБ.
import type { CSSProperties } from 'react';
import { useT } from '../i18n';
import { DB_MAX, DB_MIN, DB_STEP, dbToGain, gainToDb } from '../volume';

const pos = (db: number) => ((db - DB_MIN) / (DB_MAX - DB_MIN)) * 100;

export default function VolumeSlider({ gain, onGain, label }: { gain: number; onGain: (gain: number) => void; label?: string }) {
  const t = useT();
  const db = gainToDb(gain);
  return (
    <div className="volume-slider" style={{ '--zero': pos(0) / 100 } as CSSProperties}>
      <input
        type="range"
        min={DB_MIN}
        max={DB_MAX}
        step={DB_STEP}
        value={db}
        aria-label={label}
        title={t('param.volumeHint')}
        style={{ '--fill': `${pos(db)}%` } as CSSProperties}
        onChange={(e) => onGain(dbToGain(Number(e.target.value)))}
        onDoubleClick={() => onGain(1)}
      />
    </div>
  );
}
