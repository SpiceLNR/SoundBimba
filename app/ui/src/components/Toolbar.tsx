// SPDX-License-Identifier: AGPL-3.0-or-later
// Верхняя панель — одна строка: слева работа с картой, справа «Проект», «Клавиши» и «Настройки»
// (язык и тема).
import type { Settings } from '../api';
import { CATEGORY_ORDER, KIND_ORDER, KINDS, type NodeKind } from '../graph';
import { formatCombo, type Bindings } from '../hotkeys';
import { LANGUAGES, isLang, useT, type Lang } from '../i18n';
import { THEMES } from '../theme';
import Dropdown from './Dropdown';

export type SaveState = 'saved' | 'saving' | 'error';

interface Props {
  lang: Lang;
  settings: Settings;
  saveState: SaveState;
  bindings: Bindings;
  onAdd: (kind: NodeKind) => void;
  onArrange: () => void;
  onExport: () => void;
  onImport: () => void;
  onDiagnostics: () => void;
  onHotkeys: () => void;
  onSettings: (s: Settings) => void;
}

export default function Toolbar(p: Props) {
  const t = useT();
  const projectActions: Record<string, () => void> = {
    import: p.onImport,
    export: p.onExport,
    diagnostics: p.onDiagnostics,
  };
  return (
    <header className="toolbar">
      <div className="brand">{t('app.title')}</div>
      <Dropdown
        className="add-node"
        label={t('toolbar.add')}
        matchWidth
        onSelect={(k) => p.onAdd(k as NodeKind)}
        groups={CATEGORY_ORDER.map((cat) => ({
          title: t(`category.${cat}`),
          items: KIND_ORDER.filter((k) => KINDS[k].category === cat).map((k) => ({
            value: k,
            label: t(`node.${k}.title`),
            // Клавиша слева, в цвете узла этого типа.
            left: <span className={`key-badge cat-${cat}`}>{p.bindings[`add-${k}`] ? formatCombo(p.bindings[`add-${k}`]) : ''}</span>,
          })),
        }))}
      />
      <button type="button" onClick={p.onArrange}>
        {t('toolbar.arrange')}
      </button>
      <span className="toolbar-gap" />
      <span className={`save-state save-${p.saveState}`}>{t(`save.${p.saveState}`)}</span>
      <Dropdown
        label={t('toolbar.project')}
        align="right"
        onSelect={(v) => projectActions[v]?.()}
        groups={[
          {
            items: [
              { value: 'import', label: t('toolbar.import') },
              { value: 'export', label: t('toolbar.export') },
              { value: 'diagnostics', label: t('toolbar.diagnostics') },
            ],
          },
        ]}
      />
      <button type="button" onClick={p.onHotkeys}>
        {t('toolbar.hotkeys')}
      </button>
      <Dropdown
        className="settings-menu"
        label={t('toolbar.settings')}
        matchWidth
        align="right"
        onSelect={(v) => {
          const [what, value] = v.split(':');
          if (what === 'lang') p.onSettings({ ...p.settings, language: isLang(value) ? value : null });
          if (what === 'theme') p.onSettings({ ...p.settings, theme: value });
        }}
        groups={[
          {
            title: t('toolbar.language'),
            items: [
              { value: 'lang:system', label: t('toolbar.languageSystem'), selected: !p.settings.language },
              ...LANGUAGES.map((l) => ({ value: `lang:${l}`, label: t(`lang.${l}`), selected: p.settings.language === l })),
            ],
          },
          {
            title: t('toolbar.theme'),
            items: THEMES.map((th) => ({
              value: `theme:${th.id}`,
              label: th.names[p.lang] ?? th.names.en,
              selected: th.id === p.settings.theme,
            })),
          },
        ]}
      />
    </header>
  );
}
