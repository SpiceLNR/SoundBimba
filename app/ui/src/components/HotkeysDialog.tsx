// SPDX-License-Identifier: AGPL-3.0-or-later
// Окно «Клавиши»: список действий, у каждого — своё сочетание; можно изменить или отключить.
import { useEffect, useState } from 'react';
import { ACTIONS, assign, DEFAULT_BINDINGS, eventToCombo, formatCombo, type Action, type Bindings } from '../hotkeys';
import { useT } from '../i18n';

interface Props {
  bindings: Bindings;
  onChange: (b: Bindings) => void;
  onClose: () => void;
}

export default function HotkeysDialog({ bindings, onChange, onClose }: Props) {
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
    <div className="dialog-backdrop">
      <div className="dialog" role="dialog" aria-modal="true" aria-labelledby="hotkeys-title">
        <h2 id="hotkeys-title" className="panel-title">
          {t('hotkeys.title')}
        </h2>
        <p className="panel-desc">{t('hotkeys.hint')}</p>
        <div className="hotkey-list">
          {ACTIONS.map((a) => (
            <div key={a} className="hotkey-row">
              <span>{t(`action.${a}`)}</span>
              <span className={`hotkey-combo${bindings[a] ? '' : ' hotkey-none'}`}>
                {capturing === a ? t('hotkeys.press') : bindings[a] ? formatCombo(bindings[a]) : t('hotkeys.none')}
              </span>
              {capturing === a ? (
                <button type="button" onClick={() => setCapturing(null)}>
                  {t('hotkeys.cancel')}
                </button>
              ) : (
                <button
                  type="button"
                  onClick={() => {
                    setDisplaced(undefined);
                    setCapturing(a);
                  }}
                >
                  {t('hotkeys.change')}
                </button>
              )}
              <button
                type="button"
                disabled={!bindings[a]}
                onClick={() => {
                  setDisplaced(undefined);
                  onChange({ ...bindings, [a]: '' });
                }}
              >
                {t('hotkeys.clear')}
              </button>
            </div>
          ))}
        </div>
        {displaced && <p className="later">{t('hotkeys.displaced', { action: t(`action.${displaced}`) })}</p>}
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
          <span className="toolbar-gap" />
          <button type="button" className="primary" onClick={onClose}>
            {t('hotkeys.close')}
          </button>
        </div>
      </div>
    </div>
  );
}
