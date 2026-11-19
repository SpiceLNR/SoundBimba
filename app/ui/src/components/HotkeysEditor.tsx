// SPDX-License-Identifier: AGPL-3.0-or-later
// Вкладка «Клавиши» в настройках: два списка — «Карта и проект» и «Добавить узел». В строке —
// название и само сочетание: щелчок по сочетанию — нажать новое, крестик — отключить
// (компактно: в узком окне строка не ломается, 2026-11-19).
import { useEffect, useState } from 'react';
import { ACTIONS, actionLabel, addedKind, assign, DEFAULT_BINDINGS, eventToCombo, formatCombo, type Action, type Bindings } from '../hotkeys';
import { useT } from '../i18n';
import IconButton from './IconButton';

interface Props {
  bindings: Bindings;
  onChange: (b: Bindings) => void;
}

const GROUPS: { title: string; actions: Action[] }[] = [
  { title: 'hotkeys.groupMap', actions: ACTIONS.filter((a) => !addedKind(a)) },
  { title: 'hotkeys.groupAdd', actions: ACTIONS.filter((a) => addedKind(a)) },
];

export default function HotkeysEditor({ bindings, onChange }: Props) {
  const t = useT();
  const [capturing, setCapturing] = useState<Action | null>(null);
  const [displaced, setDisplaced] = useState<Action | undefined>();

  // Ждём нажатия: любая клавиша (кроме одних модификаторов) становится новым сочетанием.
  useEffect(() => {
    if (!capturing) return;
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopImmediatePropagation();
      const combo = eventToCombo(e);
      if (!combo) return;
      const result = assign(bindings, capturing, combo);
      setDisplaced(result.displaced);
      onChange(result.bindings);
      setCapturing(null);
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, [capturing, bindings, onChange]);

  return (
    <>
      <p className="panel-desc">{t('hotkeys.hint')}</p>
      {GROUPS.map((g) => (
        <div key={g.title} className="hotkey-group">
          <h3 className="hotkey-group-title">{t(g.title)}</h3>
          <div className="hotkey-list">
            {g.actions.map((a) => {
              const kind = addedKind(a);
              const name = kind ? t(`node.${kind}.title`) : actionLabel(a, t);
              const now = capturing === a;
              const combo = now ? t('hotkeys.press') : bindings[a] ? formatCombo(bindings[a]) : t('hotkeys.none');
              return (
                <div key={a} className="hotkey-row">
                  <span className="hotkey-name">{name}</span>
                  <button
                    type="button"
                    className={`hotkey-combo${bindings[a] || now ? '' : ' hotkey-none'}${now ? ' capturing' : ''}`}
                    aria-label={`${actionLabel(a, t)}: ${combo}`}
                    title={t(now ? 'hotkeys.cancelHint' : 'hotkeys.changeHint')}
                    onClick={() => {
                      setDisplaced(undefined);
                      setCapturing(now ? null : a);
                    }}
                  >
                    {combo}
                  </button>
                  <IconButton
                    icon="close"
                    label={t('hotkeys.clear')}
                    disabled={!bindings[a]}
                    onClick={() => {
                      setDisplaced(undefined);
                      if (now) setCapturing(null);
                      onChange({ ...bindings, [a]: '' });
                    }}
                  />
                </div>
              );
            })}
          </div>
        </div>
      ))}
      {displaced && <p className="later">{t('hotkeys.displaced', { action: actionLabel(displaced, t) })}</p>}
      <div className="dialog-actions">
        <button
          type="button"
          onClick={() => {
            setCapturing(null);
            setDisplaced(undefined);
            onChange({ ...DEFAULT_BINDINGS });
          }}
        >
          {t('hotkeys.reset')}
        </button>
      </div>
    </>
  );
}
