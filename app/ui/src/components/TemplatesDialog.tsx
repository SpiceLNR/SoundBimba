// SPDX-License-Identifier: AGPL-3.0-or-later
// Окно «Готовые схемы»: шаблон добавляется на карту одним щелчком.
import { KINDS } from '../graph';
import { useT } from '../i18n';
import { TEMPLATES, type Template } from '../templates';

interface Props {
  onPick: (tpl: Template) => void;
  onClose: () => void;
}

export default function TemplatesDialog({ onPick, onClose }: Props) {
  const t = useT();
  return (
    <div className="dialog-backdrop">
      <div className="dialog" role="dialog" aria-modal="true" aria-labelledby="templates-title">
        <h2 id="templates-title" className="panel-title">
          {t('templates.title')}
        </h2>
        <p className="panel-desc">{t('templates.hint')}</p>
        <div className="template-list">
          {TEMPLATES.map((tpl) => (
            <div key={tpl.id} className="template-card">
              <div className="template-text">
                <strong>{t(`template.${tpl.id}.title`)}</strong>
                <span className="panel-desc">{t(`template.${tpl.id}.desc`)}</span>
                {/* Из чего состоит схема — цветные плашки узлов. */}
                <span className="template-parts">
                  {tpl.nodes.map((n) => (
                    <span key={n.key} className={`template-part cat-${KINDS[n.kind].category}`}>
                      {t(`node.${n.kind}.title`)}
                    </span>
                  ))}
                </span>
              </div>
              <button type="button" className="primary" onClick={() => onPick(tpl)}>
                {t('templates.add')}
              </button>
            </div>
          ))}
        </div>
        <div className="dialog-actions">
          <span className="toolbar-gap" />
          <button type="button" onClick={onClose}>
            {t('templates.close')}
          </button>
        </div>
      </div>
    </div>
  );
}
