// SPDX-License-Identifier: AGPL-3.0-or-later
import { useEffect } from 'react';
import { useT } from '../i18n';

export interface Notice {
  id: number;
  key: string;
  vars?: Record<string, string | number>;
  error: boolean;
}

export type Notify = (key: string, vars?: Record<string, string | number>, error?: boolean) => void;

const SHOW_MS = 7000;

function NoticeItem({ notice, onClose }: { notice: Notice; onClose: (id: number) => void }) {
  const t = useT();
  useEffect(() => {
    // Ошибки висят, пока их не закроют; обычные сообщения исчезают сами.
    if (notice.error) return;
    const h = setTimeout(() => onClose(notice.id), SHOW_MS);
    return () => clearTimeout(h);
  }, [notice, onClose]);
  return (
    <div className={`notice${notice.error ? ' notice-error' : ''}`} role={notice.error ? 'alert' : 'status'}>
      <span>{t(notice.key, notice.vars)}</span>
      <button type="button" className="notice-close" onClick={() => onClose(notice.id)} aria-label={t('notice.close')}>
        ×
      </button>
    </div>
  );
}

export default function Notices({ items, onClose }: { items: Notice[]; onClose: (id: number) => void }) {
  return (
    <div className="notices">
      {items.map((n) => (
        <NoticeItem key={n.id} notice={n} onClose={onClose} />
      ))}
    </div>
  );
}
