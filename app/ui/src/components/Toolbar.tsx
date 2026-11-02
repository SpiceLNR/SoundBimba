// SPDX-License-Identifier: AGPL-3.0-or-later
// Верхняя панель — одна строка: слева работа с картой, справа меню «Проект», язык и тема.
import { useEffect, useRef, useState } from 'react';
import type { Settings } from '../api';
import { CATEGORY_ORDER, KIND_ORDER, KINDS, type NodeKind } from '../graph';
import { LANGUAGES, isLang, useT, type Lang } from '../i18n';
import { THEMES } from '../theme';

export type SaveState = 'saved' | 'saving' | 'error';

interface Props {
  lang: Lang;
  settings: Settings;
  saveState: SaveState;
  onAdd: (kind: NodeKind) => void;
  onArrange: () => void;
  onExport: () => void;
  onImport: () => void;
  onDiagnostics: () => void;
  onHotkeys: () => void;
  onSettings: (s: Settings) => void;
}

/** Меню «Проект»: импорт, экспорт, диагностика. Закрывается после выбора или щелчка мимо. */
function ProjectMenu({ items }: { items: { label: string; action: () => void }[] }) {
  const t = useT();
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
    <div className="menu" ref={ref}>
      <button type="button" aria-haspopup="menu" aria-expanded={open} onClick={() => setOpen((o) => !o)}>
        {t('toolbar.project')} ▾
      </button>
      {open && (
        <div className="menu-list" role="menu">
          {items.map((it) => (
            <button
              key={it.label}
              type="button"
              role="menuitem"
              onClick={() => {
                setOpen(false);
                it.action();
              }}
            >
              {it.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

export default function Toolbar(p: Props) {
  const t = useT();
  return (
    <header className="toolbar">
      <div className="brand">{t('app.title')}</div>
      <select
        className="add-node"
        value=""
        aria-label={t('toolbar.add')}
        onChange={(e) => {
          if (e.target.value) p.onAdd(e.target.value as NodeKind);
        }}
      >
        <option value="">{t('toolbar.add')}</option>
        {CATEGORY_ORDER.map((cat) => (
          <optgroup key={cat} label={t(`category.${cat}`)}>
            {KIND_ORDER.filter((k) => KINDS[k].category === cat).map((k) => (
              <option key={k} value={k}>
                {t(`node.${k}.title`)}
              </option>
            ))}
          </optgroup>
        ))}
      </select>
      <button type="button" onClick={p.onArrange}>
        {t('toolbar.arrange')}
      </button>
      <span className="toolbar-gap" />
      <span className={`save-state save-${p.saveState}`}>{t(`save.${p.saveState}`)}</span>
      <ProjectMenu
        items={[
          { label: t('toolbar.import'), action: p.onImport },
          { label: t('toolbar.export'), action: p.onExport },
          { label: t('toolbar.diagnostics'), action: p.onDiagnostics },
        ]}
      />
      <button type="button" onClick={p.onHotkeys}>
        {t('toolbar.hotkeys')}
      </button>
      <select
        className="lang-select"
        value={p.settings.language ?? ''}
        aria-label={t('toolbar.language')}
        title={t('toolbar.language')}
        onChange={(e) => p.onSettings({ ...p.settings, language: isLang(e.target.value) ? e.target.value : null })}
      >
        <option value="">{t('toolbar.languageSystem')}</option>
        {LANGUAGES.map((l) => (
          <option key={l} value={l}>
            {t(`lang.${l}`)}
          </option>
        ))}
      </select>
      <select
        className="theme-select"
        value={p.settings.theme}
        aria-label={t('toolbar.theme')}
        title={t('toolbar.theme')}
        onChange={(e) => p.onSettings({ ...p.settings, theme: e.target.value })}
      >
        {THEMES.map((th) => (
          <option key={th.id} value={th.id}>
            {`${t('toolbar.theme')}: ${th.names[p.lang] ?? th.names.en}`}
          </option>
        ))}
      </select>
    </header>
  );
}
