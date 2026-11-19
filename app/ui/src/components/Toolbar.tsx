// SPDX-License-Identifier: AGPL-3.0-or-later
// Верхняя панель — одна строка: слева работа с картой, справа «Проект» и «Параметры» (режим,
// кнопочная панель и «Настройки…» — окно: язык, тема, звук, клавиши).
import type { Settings } from '../api';
import { CATEGORY_ORDER, KIND_ORDER, KINDS, lookOf, type NodeKind } from '../graph';
import { formatCombo, type Bindings } from '../hotkeys';
import { useT } from '../i18n';
import Dropdown from './Dropdown';
import WindowControls, { dragWindow, dragWindowTouch, maximizeOnDoubleTap } from './WindowControls';

export type SaveState = 'saved' | 'saving' | 'error';

interface Props {
  settings: Settings;
  saveState: SaveState;
  bindings: Bindings;
  onToggleAudio: () => void;
  onAdd: (kind: NodeKind) => void;
  onArrange: () => void;
  onExport: () => void;
  onImport: () => void;
  onDiagnostics: () => void;
  onTemplates: () => void;
  onClear: () => void;
  onSettings: (s: Settings) => void;
  onOpenSettings: () => void;
  onOpenPanel: () => void;
  /** Простой режим: без кнопок работы с картой. */
  simple: boolean;
  /** Открыта микросхема: в списке появляются её ножки. */
  inChip: boolean;
}

export default function Toolbar(p: Props) {
  const t = useT();
  const projectActions: Record<string, () => void> = {
    import: p.onImport,
    export: p.onExport,
    diagnostics: p.onDiagnostics,
    templates: p.onTemplates,
    clear: p.onClear,
  };
  return (
    // У окна нет системной рамки: пустое место панели — его заголовок (`app-region` в app.css;
    // не сработал — запасной путь в WindowControls).
    <header className="toolbar" data-drag onMouseDown={dragWindow} onPointerDown={dragWindowTouch} onDoubleClick={maximizeOnDoubleTap}>
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
        groups={[...CATEGORY_ORDER, 'chip' as const].map((group) => ({
          title: t(`category.${group}`),
          // Микросхемы — своей группой в конце; ножки — только когда микросхема открыта.
          items: KIND_ORDER.filter((k) => lookOf(k) === group && (!KINDS[k].insideChip || p.inChip)).map((k) => ({
            value: k,
            label: t(`node.${k}.title`),
            // Клавиша слева, в цвете узла этого типа.
            left: <span className={`key-badge cat-${group}`}>{p.bindings[`add-${k}`] ? formatCombo(p.bindings[`add-${k}`]) : ''}</span>,
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
              { value: 'import', label: t('toolbar.import') },
              { value: 'export', label: t('toolbar.export') },
              { value: 'diagnostics', label: t('toolbar.diagnostics') },
              { value: 'clear', label: t('toolbar.clear') },
            ],
          },
        ]}
      />
      <Dropdown
        label={t('toolbar.params')}
        fit
        align="right"
        onSelect={(v) => {
          const [what, value] = v.split(':');
          if (what === 'mode') p.onSettings({ ...p.settings, mode: value === 'simple' ? 'simple' : 'expert' });
          if (what === 'window') p.onOpenPanel();
          if (what === 'settings') p.onOpenSettings();
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
          { items: [{ value: 'settings:open', label: t('toolbar.settings') }] },
        ]}
      />
      <WindowControls toTray={p.settings.closeToTray !== false} />
    </header>
  );
}
