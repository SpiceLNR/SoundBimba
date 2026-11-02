// SPDX-License-Identifier: AGPL-3.0-or-later
// Кнопочная панель — отдельное маленькое окно поверх остальных. Без горячих клавиш.
// Проект и настройки берёт у основного окна и следит за изменениями (события Tauri).
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { useCallback, useEffect, useState } from 'react';
import { api, onAudioState, type AudioState, type Settings } from './api';
import { AudioContext, EMPTY_AUDIO } from './audio';
import ButtonGrid from './components/ButtonGrid';
import type { Project } from './graph';
import { I18nContext, isLang, systemLanguage, translate, type Translate } from './i18n';
import { applyTheme } from './theme';

const inTauri = '__TAURI_INTERNALS__' in window;

export default function PanelApp() {
  const [project, setProject] = useState<Project | null>(null);
  const [settings, setSettings] = useState<Settings>({ language: null, theme: 'bimba' });
  const [audio, setAudio] = useState<AudioState>(EMPTY_AUDIO);
  const lang = isLang(settings.language) ? settings.language : systemLanguage();
  const t = useCallback<Translate>((key, vars) => translate(lang, key, vars), [lang]);

  useEffect(() => applyTheme(settings.theme), [settings.theme]);
  useEffect(() => {
    document.documentElement.lang = lang;
    document.title = t('buttons.title');
  }, [lang, t]);
  useEffect(() => onAudioState(setAudio), []);
  useEffect(() => {
    void api.getUiState().then((s) => {
      setProject(s.project);
      setSettings(s.settings);
    });
    if (!inTauri) return;
    const offs = [
      listen<Project>('project-changed', (e) => setProject(e.payload)),
      listen<Settings>('settings-changed', (e) => setSettings(e.payload)),
    ];
    return () => offs.forEach((p) => void p.then((off) => off()));
  }, []);

  const frame = settings.panelFrame !== false;
  const toggleFrame = async () => {
    const next = { ...settings, panelFrame: !frame };
    setSettings(next);
    if (inTauri) await getCurrentWindow().setDecorations(!frame);
    await api.saveSettings(next);
  };

  return (
    <I18nContext.Provider value={t}>
      <AudioContext.Provider value={audio}>
        <div className="panel-app">
          {/* За эту полосу окно можно перетаскивать, даже без рамки. */}
          <header className="panel-header" data-tauri-drag-region>
            <span className="panel-header-title" data-tauri-drag-region>
              {t('buttons.title')}
            </span>
            <button type="button" className="panel-header-button" onClick={() => void toggleFrame()}>
              {t(frame ? 'buttons.frameOff' : 'buttons.frameOn')}
            </button>
            <button
              type="button"
              className="panel-header-button"
              aria-label={t('notice.close')}
              title={t('notice.close')}
              onClick={() => void (inTauri ? getCurrentWindow().close() : window.close())}
            >
              ×
            </button>
          </header>
          {!audio.enabled && <p className="panel-hint panel-note">{t('buttons.soundOff')}</p>}
          <div className="panel-pad">{project && <ButtonGrid nodes={project.nodes} />}</div>
        </div>
      </AudioContext.Provider>
    </I18nContext.Provider>
  );
}
