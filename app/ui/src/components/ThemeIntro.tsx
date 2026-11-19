// SPDX-License-Identifier: AGPL-3.0-or-later
// Заставка темы (сейчас — «Night Hub», `intro: true` в theme.json): при включении темы и при
// запуске с ней на всё поле карты под свой бас-рифф сменяются значки (микрофон, наушники, волна,
// «▶»), потом надпись «Night» и «HUB» на плашке с подписью, и всё гаснет. Щелчок — пропустить.
// Время и вид — в theme.css темы (классы intro-*); здесь только разметка и звук.
import { type CSSProperties, useEffect, useState } from 'react';
import { previewShot } from '../api';
import { useT } from '../i18n';
import { playIntroSound } from '../introSound';
import { THEMES } from '../theme';

/** Сколько длится заставка вместе с угасанием (как в theme.css). */
const INTRO_MS = 3900;

const ICONS = [
  // Микрофон.
  'M12 3a3 3 0 0 0-3 3v6a3 3 0 0 0 6 0V6a3 3 0 0 0-3-3zM6 11a6 6 0 0 0 12 0M12 17v4M8.5 21h7',
  // Наушники.
  'M4 15v-3a8 8 0 0 1 16 0v3M4 15h3v5H4zM17 15h3v5h-3z',
  // Звуковая волна.
  'M3 12h2M7 8v8M11 4v16M15 7v10M19 10v4M21 12h0',
  // «▶» в круге.
  'M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM10 8.5v7l6-3.5z',
];

/** Стрелка курсора мыши. */
const CURSOR = 'M5 3l13 9.5-6 1.2 3.6 6.4-2.7 1.5-3.6-6.5L5 19z';

export default function ThemeIntro({ theme }: { theme: string }) {
  const t = useT();
  const enabled = THEMES.find((th) => th.id === theme)?.intro === true && !previewShot;
  const [run, setRun] = useState(0);

  useEffect(() => {
    if (!enabled) {
      setRun(0);
      return;
    }
    setRun((r) => r + 1);
    playIntroSound();
    const h = window.setTimeout(() => setRun(0), INTRO_MS);
    return () => window.clearTimeout(h);
    // Заново — только при смене темы.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [theme]);

  if (!enabled || run === 0) return null;
  const word = [...t('intro.word')];
  const sub = [...t('intro.sub')];
  return (
    <div key={run} className="theme-intro" onClick={() => setRun(0)} title={t('intro.skip')} aria-hidden="true">
      {ICONS.map((d, i) => (
        <svg key={d} className={`intro-icon intro-icon-${i + 1}`} viewBox="0 0 24 24" aria-hidden="true">
          <path d={d} />
        </svg>
      ))}
      {/* Курсор подъезжает к «▶» и нажимает (под щелчок мыши в звуке), от нажатия — кольцо. */}
      <span className="intro-ripple" />
      <svg className="intro-cursor" viewBox="0 0 24 24" aria-hidden="true">
        <path d={CURSOR} />
      </svg>
      <div className="intro-mark">
        <span className="intro-word">
          {word.map((c, i) => (
            <span key={i} style={{ '--i': i } as CSSProperties}>
              {c}
            </span>
          ))}
        </span>
        <span className="intro-plaque">{t('intro.plaque')}</span>
      </div>
      <div className="intro-sub">
        {sub.map((c, i) => (
          <span key={i} style={{ '--i': i } as CSSProperties}>
            {c}
          </span>
        ))}
      </div>
    </div>
  );
}
