// SPDX-License-Identifier: AGPL-3.0-or-later
// Окно «Готовые схемы»: встроенные шаблоны и свои (файлы .bmb в папке программы). Свой шаблон
// можно сохранить из текущей карты, открыть из файла, сохранить в файл (поделиться) и удалить.
import { open, save } from '@tauri-apps/plugin-dialog';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { api, type UserTemplate } from '../api';
import { KINDS, type NodeKind, type Project } from '../graph';
import { useT } from '../i18n';
import { fromProject, TEMPLATES, type Template } from '../templates';
import type { Notify } from './Notices';

interface Props {
  /** Текущая карта — для «Сохранить схему как шаблон». */
  project: Project;
  /** Только что открытые двойным щелчком — показать отметку «новый». */
  fresh: string[];
  /** Убранные встроенные шаблоны. */
  hidden: string[];
  onHidden: (ids: string[]) => void;
  onPick: (tpl: Template, name: string) => void;
  onClose: () => void;
  notify: Notify;
  reason: (e: unknown) => string;
}

function Parts({ kinds }: { kinds: NodeKind[] }) {
  const t = useT();
  return (
    <span className="template-parts">
      {kinds.map((k, i) => (
        <span key={i} className={`template-part cat-${KINDS[k].category}`}>
          {t(`node.${k}.title`)}
        </span>
      ))}
    </span>
  );
}

export default function TemplatesDialog({ project, fresh, hidden, onHidden, onPick, onClose, notify, reason }: Props) {
  const t = useT();
  const [mine, setMine] = useState<UserTemplate[]>([]);
  const [form, setForm] = useState<{ name: string; description: string } | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<string | null>(null);
  const filters = useMemo(() => [{ name: t('dialog.templateFilter'), extensions: ['bmb'] }], [t]);

  const reload = useCallback(() => {
    api
      .listTemplates()
      .then(setMine)
      .catch(() => setMine([]));
  }, []);
  useEffect(reload, [reload]);

  const saveCurrent = async () => {
    if (!form) return;
    try {
      const saved = await api.saveTemplate(form.name, form.description, project);
      setForm(null);
      reload();
      notify('notice.templateSaved', { name: saved.name });
    } catch (e) {
      notify('error.templateSave', { reason: reason(e) }, true);
    }
  };

  const openFile = async () => {
    const path = await open({ multiple: false, directory: false, filters });
    if (typeof path !== 'string') return;
    try {
      const added = await api.importTemplate(path);
      reload();
      notify('notice.templateImported', { name: added.name });
    } catch (e) {
      notify('error.templateOpen', { reason: reason(e) }, true);
    }
  };

  const exportFile = async (tpl: UserTemplate) => {
    const path = await save({ defaultPath: `${tpl.id}.bmb`, filters });
    if (!path) return;
    try {
      await api.exportTemplate(tpl.id, path);
      notify('notice.templateExported');
    } catch (e) {
      notify('error.templateExport', { reason: reason(e) }, true);
    }
  };

  const remove = async (tpl: UserTemplate) => {
    setConfirmDelete(null);
    try {
      await api.deleteTemplate(tpl.id);
      reload();
      notify('notice.templateDeleted', { name: tpl.name });
    } catch (e) {
      notify('error.templateDelete', { reason: reason(e) }, true);
    }
  };

  const mapEmpty = project.nodes.length === 0;
  const builtin = TEMPLATES.filter((tpl) => !hidden.includes(tpl.id));
  const hiddenCount = TEMPLATES.length - builtin.length;

  return (
    <div className="dialog-backdrop">
      <div className="dialog" role="dialog" aria-modal="true" aria-labelledby="templates-title">
        <h2 id="templates-title" className="panel-title">
          {t('templates.title')}
        </h2>
        <p className="panel-desc">{t('templates.hint')}</p>

        <h3 className="template-section">{t('templates.mine')}</h3>
        <div className="template-tools">
          <button
            type="button"
            disabled={mapEmpty || form !== null}
            title={mapEmpty ? t('templates.saveCurrentEmpty') : undefined}
            onClick={() => setForm({ name: '', description: '' })}
          >
            {t('templates.saveCurrent')}
          </button>
          <button type="button" onClick={() => void openFile()}>
            {t('templates.open')}
          </button>
        </div>
        {form && (
          <form
            className="template-form"
            onSubmit={(e) => {
              e.preventDefault();
              void saveCurrent();
            }}
          >
            <label className="field">
              <span>{t('templates.name')}</span>
              <input
                autoFocus
                maxLength={80}
                value={form.name}
                onChange={(e) => setForm({ ...form, name: e.target.value })}
              />
            </label>
            <label className="field">
              <span>{t('templates.description')}</span>
              <textarea
                rows={2}
                maxLength={400}
                value={form.description}
                onChange={(e) => setForm({ ...form, description: e.target.value })}
              />
            </label>
            <div className="dialog-actions">
              <span className="toolbar-gap" />
              <button type="button" onClick={() => setForm(null)}>
                {t('templates.cancel')}
              </button>
              <button type="submit" className="primary" disabled={!form.name.trim()}>
                {t('templates.save')}
              </button>
            </div>
          </form>
        )}
        <div className="template-list">
          {mine.length === 0 && <p className="later">{t('templates.mineEmpty')}</p>}
          {mine.map((tpl) => (
            <div key={tpl.id} className="template-card">
              <div className="template-text">
                <strong>
                  {tpl.name}
                  {fresh.includes(tpl.id) && <span className="template-new">{t('templates.new')}</span>}
                </strong>
                {tpl.description && <span className="panel-desc">{tpl.description}</span>}
                <Parts kinds={tpl.project.nodes.map((n) => n.type)} />
                {confirmDelete === tpl.id && (
                  <span className="template-confirm">
                    <span>{t('templates.deleteConfirm', { name: tpl.name })}</span>
                    <button type="button" autoFocus onClick={() => setConfirmDelete(null)}>
                      {t('templates.cancel')}
                    </button>
                    <button type="button" className="danger" onClick={() => void remove(tpl)}>
                      {t('templates.deleteYes')}
                    </button>
                  </span>
                )}
              </div>
              <div className="template-buttons">
                <button
                  type="button"
                  className="primary"
                  disabled={tpl.project.nodes.length === 0}
                  onClick={() => onPick(fromProject(tpl.id, tpl.project), tpl.name)}
                >
                  {t('templates.add')}
                </button>
                <button type="button" title={t('templates.exportTitle')} onClick={() => void exportFile(tpl)}>
                  {t('templates.export')}
                </button>
                <button type="button" onClick={() => setConfirmDelete(tpl.id)}>
                  {t('templates.delete')}
                </button>
              </div>
            </div>
          ))}
        </div>

        <div className="template-section-row">
          <h3 className="template-section">{t('templates.builtin')}</h3>
          {hiddenCount > 0 && (
            <button
              type="button"
              onClick={() => {
                onHidden([]);
                notify('notice.templatesRestored');
              }}
            >
              {t('templates.restore', { count: hiddenCount })}
            </button>
          )}
        </div>
        <div className="template-list">
          {builtin.length === 0 && <p className="later">{t('templates.builtinEmpty')}</p>}
          {builtin.map((tpl) => (
            <div key={tpl.id} className="template-card">
              <div className="template-text">
                <strong>{t(`template.${tpl.id}.title`)}</strong>
                <span className="panel-desc">{t(`template.${tpl.id}.desc`)}</span>
                <Parts kinds={tpl.nodes.map((n) => n.kind)} />
                {confirmDelete === `builtin:${tpl.id}` && (
                  <span className="template-confirm">
                    <span>{t('templates.deleteBuiltinConfirm', { name: t(`template.${tpl.id}.title`) })}</span>
                    <button type="button" autoFocus onClick={() => setConfirmDelete(null)}>
                      {t('templates.cancel')}
                    </button>
                    <button
                      type="button"
                      className="danger"
                      onClick={() => {
                        setConfirmDelete(null);
                        onHidden([...hidden, tpl.id]);
                        notify('notice.templateDeleted', { name: t(`template.${tpl.id}.title`) });
                      }}
                    >
                      {t('templates.deleteYes')}
                    </button>
                  </span>
                )}
              </div>
              <div className="template-buttons">
                <button type="button" className="primary" onClick={() => onPick(tpl, t(`template.${tpl.id}.title`))}>
                  {t('templates.add')}
                </button>
                <button type="button" onClick={() => setConfirmDelete(`builtin:${tpl.id}`)}>
                  {t('templates.delete')}
                </button>
              </div>
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
