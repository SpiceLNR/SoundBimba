// SPDX-License-Identifier: AGPL-3.0-or-later
// Круглая кнопка-звук: нажатие — играть, повторное — стоп; кольцо показывает, сколько прозвучало.
// Используется на карте, в кнопочной панели и в простом режиме.
import { useT } from '../i18n';

interface Props {
  /** Доля прозвучавшего (0…1), если играет; undefined — не играет. */
  progress?: number;
  onPress: () => void;
  size?: number;
  disabled?: boolean;
  label?: string;
}

const STROKE = 4;

export default function ButtonDisc({ progress, onPress, size = 40, disabled, label }: Props) {
  const t = useT();
  const playing = progress !== undefined;
  const r = size / 2 - STROKE;
  const length = 2 * Math.PI * r;
  return (
    <button
      type="button"
      className={`button-disc nodrag${playing ? ' playing' : ''}`}
      style={{ width: size, height: size }}
      disabled={disabled}
      aria-label={label ? `${label}: ${t(playing ? 'button.stop' : 'button.play')}` : t(playing ? 'button.stop' : 'button.play')}
      title={t(playing ? 'button.stop' : 'button.play')}
      onClick={(e) => {
        e.stopPropagation();
        onPress();
      }}
      onPointerDown={(e) => e.stopPropagation()}
    >
      <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} aria-hidden="true">
        <circle className="disc-track" cx={size / 2} cy={size / 2} r={r} strokeWidth={STROKE} />
        {playing && (
          <circle
            className="disc-progress"
            cx={size / 2}
            cy={size / 2}
            r={r}
            strokeWidth={STROKE}
            strokeDasharray={`${length * Math.min(1, Math.max(0, progress))} ${length}`}
            transform={`rotate(-90 ${size / 2} ${size / 2})`}
          />
        )}
        {playing ? (
          <rect className="disc-icon" x={size * 0.36} y={size * 0.36} width={size * 0.28} height={size * 0.28} rx={size * 0.03} />
        ) : (
          <path
            className="disc-icon"
            d={`M ${size * 0.4} ${size * 0.32} L ${size * 0.7} ${size / 2} L ${size * 0.4} ${size * 0.68} Z`}
          />
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
