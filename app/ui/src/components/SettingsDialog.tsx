// SPDX-License-Identifier: AGPL-3.0-or-later
// Окно «Настройки» (2026-11-19): то, что настраивают один раз, — по четырём вкладкам. Открывается
// из меню «Параметры»; там же — часто переключаемое (режим, кнопочная панель). Выбрали тему —
// окно закрывается (просьба автора).
import { useState } from 'react';
import { api, isCmdError, type Settings } from '../api';
import type { Bindings } from '../hotkeys';
import { LANGUAGES, isLang, useT, type Lang } from '../i18n';
import { THEMES } from '../theme';
import HotkeysEditor from './HotkeysEditor';
import IconButton from './IconButton';

type Tab = 'general' | 'theme' | 'sound' | 'keys';
const TABS: Tab[] = ['general', 'theme', 'sound', 'keys'];

// Снимки тем (tools/theme-previews.ps1) — лежат в папках тем и входят в программу.
const shots = import.meta.glob<string>('../../../../themes/*/preview.jpg', { eager: true, query: '?url', import: 'default' });
const shotOf = (id: string) => Object.entries(shots).find(([path]) => path.includes(`/themes/${id}/`))?.[1];

interface Props {
  lang: Lang;
  settings: Settings;
  bindings: Bindings;
  onSettings: (s: Settings) => void;
  onBindings: (b: Bindings) => void;
  onCable: () => void;
  onClose: () => void;
}

/** Выбор из нескольких вариантов — кнопками, выбранная подсвечена. */
function Choice<T extends string>({ value, options, onChange }: { value: T; options: { value: T; label: string; hint?: string }[]; onChange: (v: T) => void }) {
  return (
    <div className="settings-choice" role="radiogroup">
      {options.map((o) => (
        <button key={o.value} type="button" role="radio" aria-checked={o.value === value} className={o.value === value ? 'primary' : undefined} onClick={() => onChange(o.value)}>
          <span>{o.label}</span>
          {o.hint && <span className="settings-choice-hint">{o.hint}</span>}
        </button>
      ))}
    </div>
  );
}

function LatencyCheck() {
  const t = useT();
  const [state, setState] = useState<{ text: string; error: boolean } | null>(null);
  const [busy, setBusy] = useState(false);
  const run = async () => {
    setBusy(true);
    setState({ text: t('notice.latencyMeasuring'), error: false });
    try {
      const l = await api.measureLatency();
      setState({ text: t('notice.latency', { output: Math.round(l.outputMs), total: Math.round(l.outputMs + l.inputEstimateMs) }), error: false });
    } catch (e) {
      setState({ text: t(isCmdError(e) && e.code === 'no-cable' ? 'error.latencyNoCable' : 'error.latency'), error: true });
    } finally {
      setBusy(false);
    }
  };
  return (
    <>
      <button type="button" disabled={busy} onClick={() => void run()}>
        {t('toolbar.latency')}
      </button>
      {state && <p className={state.error ? 'problem-text' : 'panel-hint'}>{state.text}</p>}
    </>
  );
}

export default function SettingsDialog({ lang, settings, bindings, onSettings, onBindings, onCable, onClose }: Props) {
  const t = useT();
  const [tab, setTab] = useState<Tab>('general');
  const set = (patch: Partial<Settings>) => onSettings({ ...settings, ...patch });
  const f0 = settings.voiceF0 ?? null;

  return (
    <div className="dialog-backdrop">
      <div className="dialog settings-dialog" role="dialog" aria-modal="true" aria-labelledby="settings-title">
        <div className="dialog-head">
          <h2 id="settings-title" className="panel-title">
            {t('settings.title')}
          </h2>
          <IconButton icon="close" label={t('settings.close')} onClick={onClose} />
        </div>
        <div className="settings-body">
          <nav className="settings-tabs" role="tablist" aria-orientation="vertical">
            {TABS.map((id) => (
              <button
                key={id}
                type="button"
                role="tab"
                aria-selected={tab === id}
                className={tab === id ? 'primary' : undefined}
                onClick={() => setTab(id)}
              >
                {t(`settings.tab.${id}`)}
              </button>
            ))}
          </nav>
          <section className="settings-page" role="tabpanel" aria-label={t(`settings.tab.${tab}`)}>
            {tab === 'general' && (
              <>
                <div className="field">
                  <span>{t('toolbar.language')}</span>
                  <Choice
                    value={isLang(settings.language) ? settings.language : 'system'}
                    options={[{ value: 'system', label: t('toolbar.languageSystem') }, ...LANGUAGES.map((l) => ({ value: l, label: t(`lang.${l}`) }))]}
                    onChange={(v) => set({ language: isLang(v) ? v : null })}
                  />
                </div>
                <div className="field">
                  <span>{t('buttons.open')}</span>
                  <label className="check">
                    <input type="checkbox" checked={settings.panelFrame !== false} onChange={(e) => set({ panelFrame: e.target.checked })} />
                    <span>{t('settings.panelFrame')}</span>
                  </label>
                </div>
                <div className="field">
                  <span>{t('settings.recordings')}</span>
                  <p className="panel-hint">{t('settings.recordingsHint')}</p>
                  <div className="settings-row">
                    <button type="button" onClick={() => void api.openRecordings()}>
                      {t('settings.recordingsOpen')}
                    </button>
                  </div>
                </div>
              </>
            )}
            {tab === 'theme' && (
              <>
                <p className="panel-hint">{t('settings.themeHint')}</p>
                <div className="theme-grid">
                  {THEMES.map((th) => {
                    const shot = shotOf(th.id);
                    const name = th.names[lang] ?? th.names.en;
                    const on = th.id === settings.theme;
                    return (
                      <button
                        key={th.id}
                        type="button"
                        className={`theme-card${on ? ' on' : ''}`}
                        aria-pressed={on}
                        onClick={() => {
                          set({ theme: th.id });
                          onClose();
                        }}
                      >
                        {shot ? <img src={shot} alt="" draggable={false} /> : <span className="theme-card-empty" data-theme={th.id} />}
                        <span className="theme-card-name">{name}</span>
                      </button>
                    );
                  })}
                </div>
              </>
            )}
            {tab === 'sound' && (
              <>
                <div className="field">
                  <span>{t('toolbar.vr')}</span>
                  <Choice
                    value={settings.vrSteady === false ? 'fast' : 'steady'}
                    options={[
                      { value: 'steady', label: t('vr.steady'), hint: t('settings.vrSteadyHint') },
                      { value: 'fast', label: t('vr.fast'), hint: t('settings.vrFastHint') },
                    ]}
                    onChange={(v) => set({ vrSteady: v === 'steady' })}
                  />
                  <p className="panel-hint">{t('settings.vrNote')}</p>
                </div>
                <div className="field">
                  <span>{t('settings.cable')}</span>
                  <p className="panel-hint">{t('settings.cableHint')}</p>
                  <div className="settings-row">
                    <button type="button" onClick={onCable}>
                      {t('toolbar.cable')}
                    </button>
                    <LatencyCheck />
                  </div>
                </div>
                <div className="field">
                  <span>{t('settings.voice')}</span>
                  <p className="panel-hint">{f0 === null ? t('voiceCal.none') : t('voiceCal.set', { hz: Math.round(f0) })}</p>
                  <p className="panel-hint">{t('settings.voiceHint')}</p>
                  {f0 !== null && (
                    <div className="settings-row">
                      <button type="button" onClick={() => set({ voiceF0: null })}>
                        {t('voiceCal.reset')}
                      </button>
                    </div>
                  )}
                </div>
              </>
            )}
            {tab === 'keys' && <HotkeysEditor bindings={bindings} onChange={onBindings} />}
          </section>
        </div>
      </div>
    </div>
  );
}
