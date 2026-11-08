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
  onToggleAudio: () => void;
  onAdd: (kind: NodeKind) => void;
  onArrange: () => void;
  onExport: () => void;
  onImport: () => void;
  onDiagnostics: () => void;
  onLatency: () => void;
  onHotkeys: () => void;
  onTemplates: () => void;
  onCable: () => void;
  onClear: () => void;
  onSettings: (s: Settings) => void;
  onOpenPanel: () => void;
  /** Простой режим: без кнопок работы с картой. */
  simple: boolean;
}

export default function Toolbar(p: Props) {
  const t = useT();
  const projectActions: Record<string, () => void> = {
    import: p.onImport,
    export: p.onExport,
    diagnostics: p.onDiagnostics,
    latency: p.onLatency,
    templates: p.onTemplates,
    cable: p.onCable,
    clear: p.onClear,
  };
  return (
    <header className="toolbar">
      <div className="brand">{t('app.title')}</div>
      <button
        type="button"
        className={`sound-toggle${p.settings.audioEnabled ? ' on' : ''}`}
        aria-pressed={Boolean(p.settings.audioEnabled)}
        title={t(p.settings.audioEnabled ? 'toolbar.soundTurnOff' : 'toolbar.soundTurnOn')}
        onClick={p.onToggleAudio}
      >
        <span className="sound-dot" aria-hidden="true" />
        {t(p.settings.audioEnabled ? 'toolbar.soundOn' : 'toolbar.soundOff')}
      </button>
      {!p.simple && (
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
      )}
      {!p.simple && (
        <button type="button" onClick={p.onArrange}>
          {t('toolbar.arrange')}
        </button>
      )}
      <span className="toolbar-gap" />
      <span className={`save-state save-${p.saveState}`}>{t(`save.${p.saveState}`)}</span>
      <Dropdown
        label={t('toolbar.project')}
        align="right"
        onSelect={(v) => projectActions[v]?.()}
        groups={[
          {
            items: [
              { value: 'templates', label: t('toolbar.templates') },
              { value: 'cable', label: t('toolbar.cable') },
              { value: 'import', label: t('toolbar.import') },
              { value: 'export', label: t('toolbar.export') },
              { value: 'latency', label: t('toolbar.latency') },
              { value: 'diagnostics', label: t('toolbar.diagnostics') },
              { value: 'clear', label: t('toolbar.clear') },
            ],
          },
        ]}
      />
      <button type="button" onClick={p.onHotkeys}>
        {t('toolbar.hotkeys')}
      </button>
      <Dropdown
        label={t('toolbar.settings')}
        fit
        align="right"
        onSelect={(v) => {
          const [what, value] = v.split(':');
          if (what === 'lang') p.onSettings({ ...p.settings, language: isLang(value) ? value : null });
          if (what === 'theme') p.onSettings({ ...p.settings, theme: value });
          if (what === 'mode') p.onSettings({ ...p.settings, mode: value === 'simple' ? 'simple' : 'expert' });
          if (what === 'window') p.onOpenPanel();
        }}
        groups={[
          {
            title: t('toolbar.mode'),
            items: [
              { value: 'mode:expert', label: t('mode.expert'), selected: p.settings.mode !== 'simple' },
              { value: 'mode:simple', label: t('mode.simple'), selected: p.settings.mode === 'simple' },
            ],
          },
          {
            title: t('toolbar.windows'),
            items: [{ value: 'window:buttons', label: t('buttons.open') }],
          },
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
