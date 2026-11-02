// SPDX-License-Identifier: AGPL-3.0-or-later
// Круглая кнопка-звук. Середина — играть / пауза / продолжить. Кольцо — шкала времени: нажатие
// на кольцо перематывает (верх — начало, по часовой стрелке — к концу), даже если кнопка стояла.
// Внутри — время: сейчас и всего. Используется на карте, в кнопочной панели и простом режиме.
import type React from 'react';
import type { ButtonPlayback } from '../api';
import { useT } from '../i18n';

interface Props {
  /** Позиция и длина звука; undefined — звука нет (файл не выбран, звук выключен). */
  state?: ButtonPlayback;
  onPress: () => void;
  onSeek: (fraction: number) => void;
  size?: number;
  disabled?: boolean;
  label?: string;
}

/** 75 → «1:15», 3725 → «1:02:05». */
export function formatTime(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const ss = String(s % 60).padStart(2, '0');
  return h > 0 ? `${h}:${String(m).padStart(2, '0')}:${ss}` : `${m}:${ss}`;
}

/** Доля звука по точке на кольце: верх — 0, по часовой стрелке до 1. */
export function ringFraction(dx: number, dy: number): number {
  const angle = Math.atan2(dx, -dy);
  return (angle < 0 ? angle + 2 * Math.PI : angle) / (2 * Math.PI);
}

export default function ButtonDisc({ state, onPress, onSeek, size = 96, disabled, label }: Props) {
  const t = useT();
  const playing = state?.playing === true;
  const paused = !playing && (state?.pos ?? 0) > 0;
  const stroke = Math.max(5, Math.round(size * 0.08));
  const c = size / 2;
  const r = c - stroke / 2 - 1;
  const length = 2 * Math.PI * r;
  const fraction = state && state.len > 0 ? Math.min(1, state.pos / state.len) : 0;
  const action = t(playing ? 'button.pause' : paused ? 'button.resume' : 'button.play');
  const icon = size * 0.17;
  const iconY = c - size * 0.2;

  const onClick = (e: React.MouseEvent<HTMLButtonElement>) => {
    e.stopPropagation();
    const box = e.currentTarget.getBoundingClientRect();
    const dx = e.clientX - (box.left + box.width / 2);
    const dy = e.clientY - (box.top + box.height / 2);
    const scale = box.width / size; // кнопка может быть уменьшена масштабом карты
    const dist = Math.hypot(dx, dy) / scale;
    // Попали по кольцу (с запасом внутрь — по нему легче попасть) — перемотка, иначе — играть/стоп.
    if (state && state.len > 0 && dist >= r - stroke * 1.2) onSeek(ringFraction(dx, dy));
    else onPress();
  };

  return (
    <button
      type="button"
      className={`button-disc nodrag${playing ? ' playing' : ''}`}
      style={{ width: size, height: size }}
      disabled={disabled}
      aria-label={label ? `${label}: ${action}` : action}
      title={`${action}. ${t('button.seekHint')}`}
      onClick={onClick}
      onPointerDown={(e) => e.stopPropagation()}
    >
      <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} aria-hidden="true">
        <circle className="disc-track" cx={c} cy={c} r={r} strokeWidth={stroke} />
        {fraction > 0 && (
          <circle
            className="disc-progress"
            cx={c}
            cy={c}
            r={r}
            strokeWidth={stroke}
            strokeDasharray={`${length * fraction} ${length}`}
            transform={`rotate(-90 ${c} ${c})`}
          />
        )}
        {playing ? (
          <>
            <rect className="disc-icon" x={c - icon * 0.45} y={iconY - icon / 2} width={icon * 0.32} height={icon} rx={icon * 0.08} />
            <rect className="disc-icon" x={c + icon * 0.13} y={iconY - icon / 2} width={icon * 0.32} height={icon} rx={icon * 0.08} />
          </>
        ) : (
          <path
            className="disc-icon"
            d={`M ${c - icon * 0.4} ${iconY - icon * 0.55} L ${c + icon * 0.6} ${iconY} L ${c - icon * 0.4} ${iconY + icon * 0.55} Z`}
          />
        )}
        {state && (
          <>
            <text className="disc-time-now" x={c} y={c + size * 0.08} fontSize={Math.max(9, size * 0.15)}>
              {formatTime(state.pos)}
            </text>
            <text className="disc-time-total" x={c} y={c + size * 0.24} fontSize={Math.max(8, size * 0.11)}>
              {formatTime(state.len)}
            </text>
          </>
        )}
      </svg>
    </button>
  );
}

/** Подпись кнопки: своё название, иначе имя файла без пути, иначе «Кнопка-звук». */
export function buttonLabel(params: Record<string, unknown>, fallback: string): string {
  if (typeof params.label === 'string' && params.label.trim()) return params.label.trim();
  if (typeof params.file === 'string' && params.file) {
    const name = params.file.split(/[\\/]/).pop() ?? params.file;
    return name.replace(/\.[^.]+$/, '');
  }
  return fallback;
}
