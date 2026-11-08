// SPDX-License-Identifier: AGPL-3.0-or-later
// Свой выпадающий список вместо системного <select>: системный рисует Windows, и его нельзя
// оформить по теме. Этот берёт скругления, тени и цвета из темы.
import { useEffect, useRef, useState, type ReactNode } from 'react';

export interface DropdownItem {
  value: string;
  label: string;
  /** Что показать слева от названия (например, клавишу в цвете узла). */
  left?: ReactNode;
  selected?: boolean;
}

export interface DropdownGroup {
  title?: string;
  items: DropdownItem[];
}

interface Props {
  /** Надпись на кнопке. */
  label: string;
  groups: DropdownGroup[];
  onSelect: (value: string) => void;
  /** Класс кнопки (ширина). */
  className?: string;
  /** Список той же ширины, что кнопка (иначе — не уже кнопки, по содержимому). */
  matchWidth?: boolean;
  /** Кнопка шириной с самый длинный пункт списка, список — ровно с кнопку (на любом языке и в
   *  любой теме: в кнопке невидимо лежат все пункты и задают её ширину). */
  fit?: boolean;
  align?: 'left' | 'right';
  ariaLabel?: string;
}

export default function Dropdown({ label, groups, onSelect, className, matchWidth, fit, align = 'left', ariaLabel }: Props) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const close = (e: PointerEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    };
    // Перехват на этапе погружения: карта React Flow сама гасит события мыши.
    window.addEventListener('pointerdown', close, true);
    return () => window.removeEventListener('pointerdown', close, true);
  }, [open]);

  return (
    <div className={`dropdown ${className ?? ''}`} ref={ref}>
      <button
        type="button"
        className="dropdown-button"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={ariaLabel}
        title={ariaLabel}
        onClick={() => setOpen((o) => !o)}
      >
        <span className={`dropdown-label${fit ? ' fit' : ''}`}>
          <span>{label}</span>
          {fit && (
            <span className="dropdown-ghost" aria-hidden="true">
              {groups.flatMap((g) => g.items).map((it) => (
                <span key={it.value}>{it.label}</span>
              ))}
            </span>
          )}
        </span>
        <span className="dropdown-caret" aria-hidden="true">
          ▾
        </span>
      </button>
      {open && (
        <div className={`dropdown-list align-${align}${matchWidth || fit ? ' match-width' : ''}`} role="listbox">
          {groups.map((g, gi) => (
            <div key={g.title ?? gi} className="dropdown-group" role="group" aria-label={g.title}>
              {g.title && <div className="dropdown-group-title">{g.title}</div>}
              {g.items.map((it) => (
                <button
                  key={it.value}
                  type="button"
                  role="option"
                  aria-selected={Boolean(it.selected)}
                  className={`dropdown-item${it.selected ? ' selected' : ''}`}
                  onClick={() => {
                    setOpen(false);
                    onSelect(it.value);
                  }}
                >
                  {it.left}
                  <span className="dropdown-item-label">{it.label}</span>
                </button>
              ))}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
