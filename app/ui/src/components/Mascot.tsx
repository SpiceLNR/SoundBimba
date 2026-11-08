// SPDX-License-Identifier: AGPL-3.0-or-later
// Персонаж темы: если в theme.json есть «mascot», при включении темы (и при запуске с ней) из-за
// правого края карты плавно выезжает картинка themes/<id>/mascot.svg, по очереди говорит фразы в
// облачке как в комиксе (овал с хвостиком к говорящему), потом уезжает. Щелчок — уехать сразу.
// Цвета облачка — в теме.
import { type CSSProperties, useEffect, useRef, useState } from 'react';
import { useT } from '../i18n';
import { THEMES } from '../theme';

const images = import.meta.glob<string>('../../../../themes/*/mascot.svg', { eager: true, query: '?url', import: 'default' });

/** Сколько миллисекунд показывать каждую фразу. */
const LINE_MS = [3600, 2600];
/** Выезд и уезд (как в app.css). */
const SLIDE_MS = 1100;

function imageOf(themeId: string): string | undefined {
  return Object.entries(images).find(([path]) => path.includes(`/themes/${themeId}/`))?.[1];
}

/** Облачко: овал и изогнутый сужающийся хвостик вниз вправо — к лицу персонажа. */
const BUBBLE = 'M192.7 111.5 A118 58 0 1 1 227.2 93 Q238 120 250 154 Q216 126 192.7 111.5 Z';

/** Короткая фраза-восклицание (например, «Нрааааааица!») — крупнее, насколько влезает в облачко. */
const SHORT = 18;

type Phase = 'hidden' | 'in' | 'out';

export default function Mascot({ theme }: { theme: string }) {
  const t = useT();
  const meta = THEMES.find((th) => th.id === theme)?.mascot;
  const image = meta ? imageOf(theme) : undefined;
  const [phase, setPhase] = useState<Phase>('hidden');
  const [line, setLine] = useState(-1);
  const timers = useRef<number[]>([]);

  const clear = () => {
    timers.current.forEach((h) => window.clearTimeout(h));
    timers.current = [];
  };
  const later = (ms: number, f: () => void) => timers.current.push(window.setTimeout(f, ms));
  const leave = () => {
    clear();
    setLine(-1);
    setPhase('out');
    later(SLIDE_MS, () => setPhase('hidden'));
  };

  useEffect(() => {
    clear();
    setLine(-1);
    if (!meta || !image) {
      setPhase('hidden');
      return clear;
    }
    // Выехать, сказать фразы по очереди, уехать.
    setPhase('in');
    let at = SLIDE_MS;
    meta.lines.forEach((_, i) => {
      later(at, () => setLine(i));
      at += LINE_MS[i] ?? LINE_MS[LINE_MS.length - 1];
    });
    later(at, leave);
    return clear;
    // Запускаем заново только при смене темы.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [theme]);

  if (!meta || !image || phase === 'hidden') return null;
  return (
    <div className={`mascot-stage ${phase}`} aria-live="polite">
      <button type="button" className="mascot" onClick={leave} title={t('mascot.hide')} aria-label={t('mascot.hide')}>
        <img src={image} alt="" draggable={false} />
        {line >= 0 && (
          <span key={line} className="mascot-bubble">
            <svg className="mascot-bubble-shape" viewBox="0 0 260 160" preserveAspectRatio="none" aria-hidden="true">
              <path d={BUBBLE} vectorEffect="non-scaling-stroke" />
            </svg>
            <span
              className={`mascot-bubble-text${t(meta.lines[line]).length <= SHORT ? ' short' : ''}`}
              style={{ '--chars': t(meta.lines[line]).length } as CSSProperties}
            >
              {t(meta.lines[line])}
            </span>
          </span>
        )}
      </button>
    </div>
  );
}
