// SPDX-License-Identifier: AGPL-3.0-or-later
// Приветствие при первом запуске: что такое карта и три пути — шаблон, кабель или сам.
import { useT } from '../i18n';

interface Props {
  onTemplates: () => void;
  onCable: () => void;
  onClose: () => void;
}

export default function WelcomeDialog({ onTemplates, onCable, onClose }: Props) {
  const t = useT();
  return (
    <div className="dialog-backdrop">
      <div className="dialog dialog-narrow" role="dialog" aria-modal="true" aria-labelledby="welcome-title">
        <h2 id="welcome-title" className="panel-title">
          {t('welcome.title')}
        </h2>
        <p className="panel-desc">{t('welcome.text')}</p>
        <div className="welcome-choices">
          <button type="button" className="primary" onClick={onTemplates}>
            {t('welcome.templates')}
          </button>
          <button type="button" onClick={onCable}>
            {t('welcome.cable')}
          </button>
          <button type="button" onClick={onClose}>
            {t('welcome.empty')}
          </button>
        </div>
      </div>
    </div>
  );
}
