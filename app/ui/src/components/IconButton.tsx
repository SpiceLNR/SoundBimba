// SPDX-License-Identifier: AGPL-3.0-or-later
// Кнопка-значок с подписью при наведении (и при переходе с клавиатуры). Значки нарисованы для
// SoundBimba — простые линии цветом текста; подпись — всплывающая, поверх всего окна (через
// portal, чтобы её не обрезала прокрутка списка).
import { useState, type ReactNode } from 'react';
import { createPortal } from 'react-dom';

const PATHS = {
  /** Добавить на карту. */
  add: 'M12 5v14M5 12h14',
  /** Обновить шаблон из карты (стрелки по кругу). */
  update: 'M19 12a7 7 0 1 1-2.05-4.95M19 4.5V8h-3.5',
  /** Сохранить в файл (стрелка из лотка). */
  export: 'M12 15V4M7.5 8.5 12 4l4.5 4.5M5 14v6h14v-6',
  /** Удалить (корзина). */
  delete: 'M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13M10 11v6M14 11v6',
  /** Закрыть окно (крестик). */
  close: 'M6 6l12 12M18 6 6 18',
} as const;

export type IconName = keyof typeof PATHS;

export function Icon({ name }: { name: IconName }) {
  return (
    <svg className="icon" viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
      <path d={PATHS[name]} fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

interface Props {
  icon: IconName;
  /** Название: подпись при наведении и для экранного диктора. */
  label: string;
  /** Подробнее, если нужно (вторая строка подписи). */
  hint?: string;
  onClick: () => void;
  disabled?: boolean;
  primary?: boolean;
  danger?: boolean;
}

export default function IconButton({ icon, label, hint, onClick, disabled, primary, danger }: Props) {
  const [tip, setTip] = useState<DOMRect | null>(null);
  const show = (el: Element) => setTip(el.getBoundingClientRect());
  const hide = () => setTip(null);
  let popup: ReactNode = null;
  if (tip) {
    // Над кнопкой; у верхнего края окна — под ней.
    const below = tip.top < 60;
    popup = createPortal(
      <div
        className="icon-tip"
        role="tooltip"
        style={{ left: tip.left + tip.width / 2, top: below ? tip.bottom + 8 : tip.top - 8 }}
        data-below={below || undefined}
      >
        <strong>{label}</strong>
        {hint && <span>{hint}</span>}
      </div>,
      document.body,
    );
  }
  return (
    // Обёртка ловит наведение и у выключенной кнопки (выключенная сама событий мыши не даёт).
    <span className="icon-button-wrap" onPointerEnter={(e) => show(e.currentTarget)} onPointerLeave={hide}>
      <button
        type="button"
        className={`icon-button${primary ? ' primary' : ''}${danger ? ' danger' : ''}`}
        aria-label={label}
        disabled={disabled}
        onClick={() => {
          hide();
          onClick();
        }}
        onFocus={(e) => show(e.currentTarget)}
        onBlur={hide}
      >
        <Icon name={icon} />
      </button>
      {popup}
    </span>
  );
}
