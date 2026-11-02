// SPDX-License-Identifier: AGPL-3.0-or-later
// Мастер «Виртуальный кабель»: есть ли кабель, как его поставить (это делает сам человек — мы
// драйвер не устанавливаем) и работает ли связка SoundBimba → кабель (с замером задержки).
import { useCallback, useEffect, useState } from 'react';
import { api, isCmdError } from '../api';
import { CABLE_URL, foundCable } from '../cable';
import { useT } from '../i18n';

type Test = { state: 'idle' | 'testing' | 'failed' } | { state: 'works'; output: number; total: number };

interface Props {
  onClose: () => void;
}

export default function CableWizard({ onClose }: Props) {
  const t = useT();
  const [cable, setCable] = useState<string | null | undefined>(undefined); // undefined — ищем
  const [copied, setCopied] = useState(false);
  const [test, setTest] = useState<Test>({ state: 'idle' });

  const check = useCallback(() => {
    setCable(undefined);
    setTest({ state: 'idle' });
    api
      .listAudioDevices()
      .then((d) => setCable(foundCable(d)))
      .catch(() => setCable(null));
  }, []);

  useEffect(check, [check]);

  const copy = () => {
    void navigator.clipboard
      ?.writeText(CABLE_URL)
      .then(() => setCopied(true))
      .catch(() => {});
  };

  const runTest = async () => {
    setTest({ state: 'testing' });
    try {
      const l = await api.measureLatency();
      setTest({ state: 'works', output: Math.round(l.outputMs), total: Math.round(l.outputMs + l.inputEstimateMs) });
    } catch (e) {
      if (isCmdError(e) && e.code === 'no-cable') setCable(null);
      setTest({ state: 'failed' });
    }
  };

  return (
    <div className="dialog-backdrop">
      <div className="dialog" role="dialog" aria-modal="true" aria-labelledby="cable-title">
        <h2 id="cable-title" className="panel-title">
          {t('cable.title')}
        </h2>
        <p className="panel-desc">{t('cable.whatFor')}</p>
        <p className={`cable-status${cable ? ' cable-ok' : cable === null ? ' cable-missing' : ''}`} role="status">
          {cable === undefined ? t('cable.checking') : cable ? t('cable.found', { name: cable }) : t('cable.notFound')}
        </p>
        {cable === null && (
          <ol className="cable-steps">
            <li>
              {t('cable.step1')}
              <span className="cable-url">
                <code>{CABLE_URL}</code>
                <button type="button" onClick={copy}>
                  {t(copied ? 'cable.copied' : 'cable.copy')}
                </button>
              </span>
            </li>
            <li>{t('cable.step2')}</li>
            <li>{t('cable.step3')}</li>
            <li>{t('cable.step4')}</li>
            <li>{t('cable.step5')}</li>
          </ol>
        )}
        {cable && (
          <>
            <p className="panel-desc">{t('cable.tip')}</p>
            {test.state === 'testing' && <p className="later">{t('cable.testing')}</p>}
            {test.state === 'works' && (
              <p className="cable-status cable-ok">{t('cable.works', { output: test.output, total: test.total })}</p>
            )}
            {test.state === 'failed' && <p className="cable-status cable-missing">{t('cable.failed')}</p>}
          </>
        )}
        <div className="dialog-actions">
          <button type="button" onClick={check} disabled={cable === undefined || test.state === 'testing'}>
            {t('cable.recheck')}
          </button>
          {cable && (
            <button type="button" className="primary" onClick={() => void runTest()} disabled={test.state === 'testing'}>
              {t('cable.test')}
            </button>
          )}
          <span className="toolbar-gap" />
          <button type="button" onClick={onClose}>
            {t('cable.close')}
          </button>
        </div>
      </div>
    </div>
  );
}
