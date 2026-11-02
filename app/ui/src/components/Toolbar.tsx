// SPDX-License-Identifier: AGPL-3.0-or-later
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
  onSettings: (s: Settings) => void;
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
      <button type="button" onClick={p.onImport}>
        {t('toolbar.import')}
      </button>
      <button type="button" onClick={p.onExport}>
        {t('toolbar.export')}
      </button>
      <button type="button" onClick={p.onDiagnostics}>
        {t('toolbar.diagnostics')}
      </button>
      <label className="toolbar-field">
        <span>{t('toolbar.language')}</span>
        <select
          value={p.settings.language ?? ''}
          onChange={(e) => p.onSettings({ ...p.settings, language: isLang(e.target.value) ? e.target.value : null })}
        >
          <option value="">{t('toolbar.languageSystem')}</option>
          {LANGUAGES.map((l) => (
            <option key={l} value={l}>
              {t(`lang.${l}`)}
            </option>
          ))}
        </select>
      </label>
      <label className="toolbar-field">
        <span>{t('toolbar.theme')}</span>
        <select value={p.settings.theme} onChange={(e) => p.onSettings({ ...p.settings, theme: e.target.value })}>
          {THEMES.map((th) => (
            <option key={th.id} value={th.id}>
              {th.names[p.lang] ?? th.names.en}
            </option>
          ))}
        </select>
      </label>
    </header>
  );
}
