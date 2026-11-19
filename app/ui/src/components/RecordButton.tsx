// SPDX-License-Identifier: AGPL-3.0-or-later
// Круглая кнопка «Диктофона»: красная точка — начать запись, квадрат — остановить.
import type { RecorderInfo } from '../api';
import type { Translate } from '../i18n';
import { useT } from '../i18n';
import { formatTime } from './ButtonDisc';

interface Props {
  recording: boolean;
  onToggle: () => void;
  size?: number;
  disabled?: boolean;
}

export default function RecordButton({ recording, onToggle, size = 56, disabled }: Props) {
  const t = useT();
  const label = t(recording ? 'recorder.stop' : 'recorder.start');
  return (
    <button
      type="button"
      className={`record-button nodrag${recording ? ' recording' : ''}`}
      style={{ width: size, height: size }}
      disabled={disabled}
      aria-label={label}
      aria-pressed={recording}
      title={label}
      onClick={(e) => {
        e.stopPropagation();
        onToggle();
      }}
      onPointerDown={(e) => e.stopPropagation()}
      onDoubleClick={(e) => e.stopPropagation()}
    >
      <span className="record-icon" aria-hidden="true" />
    </button>
  );
}

/** Подпись диктофона: «Запись 0:12», «0:34 · не сохранено», «Записи нет». */
export function recorderLabel(info: RecorderInfo | undefined, t: Translate): string {
  if (info?.recording) return t('recorder.recording', { time: formatTime(info.seconds) });
  if (info?.hasTake) return t(info.saved ? 'recorder.saved' : 'recorder.unsaved', { time: formatTime(info.seconds) });
  return t('recorder.none');
}
