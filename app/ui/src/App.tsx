// SPDX-License-Identifier: AGPL-3.0-or-later
import { ReactFlowProvider } from '@xyflow/react';
import { useCallback, useEffect, useState } from 'react';
import { api, isCmdError, type LoadState, type Settings } from './api';
import Notices, { type Notice, type Notify } from './components/Notices';
import Workspace from './components/Workspace';
import { I18nContext, isLang, systemLanguage, translate, type Translate } from './i18n';
import { applyTheme, DEFAULT_THEME, themeId } from './theme';

let noticeId = 0;

export default function App() {
  const [loaded, setLoaded] = useState<LoadState | null>(null);
  const [failed, setFailed] = useState(false);
  const [settings, setSettings] = useState<Settings>({ language: null, theme: DEFAULT_THEME });
  const [notices, setNotices] = useState<Notice[]>([]);

  const lang = isLang(settings.language) ? settings.language : systemLanguage();
  const t = useCallback<Translate>((key, vars) => translate(lang, key, vars), [lang]);

  const notify = useCallback<Notify>((key, vars, error = false) => {
    noticeId += 1;
    setNotices((list) => [...list, { id: noticeId, key, vars, error }]);
  }, []);

  useEffect(() => {
    document.documentElement.lang = lang;
  }, [lang]);

  useEffect(() => applyTheme(settings.theme), [settings.theme]);

  useEffect(() => {
    api
      .loadState()
      .then((state) => {
        setSettings({ ...state.settings, theme: themeId(state.settings.theme) });
        setLoaded(state);
        if (state.recoveredFrom) notify('notice.recovered', { file: state.recoveredFrom }, true);
      })
      .catch((e: unknown) => {
        api.log('error', `load_state failed: ${isCmdError(e) ? e.detail : String(e)}`);
        setFailed(true);
      });
  }, [notify]);

  const changeSettings = useCallback(
    (next: Settings) => {
      setSettings(next);
      api.saveSettings(next).catch(() => notify('error.save', { reason: translate(lang, 'reason.io') }, true));
    },
    [lang, notify],
  );

  return (
    <I18nContext.Provider value={t}>
      {loaded ? (
        <ReactFlowProvider>
          <Workspace
            project={loaded.project}
            version={loaded.version}
            settings={settings}
            lang={lang}
            onSettings={changeSettings}
            notify={notify}
          />
        </ReactFlowProvider>
      ) : (
        <div className="loading">{failed ? t('reason.io') : t('app.title')}</div>
      )}
      <Notices items={notices} onClose={(id) => setNotices((list) => list.filter((n) => n.id !== id))} />
    </I18nContext.Provider>
  );
}
