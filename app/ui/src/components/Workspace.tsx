// SPDX-License-Identifier: AGPL-3.0-or-later
// Рабочая среда: панель инструментов, карта (React Flow) и боковая панель. Автосохранение проекта.
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { open, save } from '@tauri-apps/plugin-dialog';
import {
  addEdge,
  Background,
  BackgroundVariant,
  Controls,
  MiniMap,
  ReactFlow,
  useEdgesState,
  useNodesState,
  useReactFlow,
  type Connection,
  type Edge,
  type OnSelectionChangeParams,
} from '@xyflow/react';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { api, isCmdError, type Settings } from '../api';
import {
  arrange,
  canConnect,
  edgeId,
  fromFlow,
  KINDS,
  newNode,
  nextNodeId,
  NODE_HEIGHT,
  NODE_WIDTH,
  toFlow,
  type NodeKind,
  type Params,
  type Project,
  type SoundNode,
} from '../graph';
import { ACTIONS, actionFor, addedKind, eventToCombo, isTyping, mergeBindings, type Action, type Bindings } from '../hotkeys';
import { DICTS, useT, type Lang } from '../i18n';
import { applyTemplate, type Template } from '../templates';
import CableWizard from './CableWizard';
import HotkeysDialog from './HotkeysDialog';
import type { Notify } from './Notices';
import SidePanel from './SidePanel';
import SimpleView from './SimpleView';
import SoundNodeView from './SoundNodeView';
import TemplatesDialog from './TemplatesDialog';
import Toolbar, { type SaveState } from './Toolbar';
import Watermark from './Watermark';
import WelcomeDialog from './WelcomeDialog';

const nodeTypes = { sound: SoundNodeView };
const SAVE_DELAY_MS = 600;

interface Props {
  project: Project;
  version: string;
  settings: Settings;
  lang: Lang;
  onSettings: (s: Settings) => void;
  onToggleAudio: () => void;
  notify: Notify;
}

export default function Workspace({ project, version, settings, lang, onSettings, onToggleAudio, notify }: Props) {
  const t = useT();
  const rf = useReactFlow<SoundNode>();
  const initial = useMemo(() => toFlow(project), [project]);
  const [nodes, setNodes, onNodesChange] = useNodesState<SoundNode>(initial.nodes);
  const [edges, setEdges, onEdgesChange] = useEdgesState<Edge>(initial.edges);
  const [selected, setSelected] = useState<{ node?: string; edge?: string }>({});
  const [saveState, setSaveState] = useState<SaveState>('saved');
  const [hotkeysOpen, setHotkeysOpen] = useState(false);
  const [templatesOpen, setTemplatesOpen] = useState(false);
  const [cableOpen, setCableOpen] = useState(false);
  const [clearOpen, setClearOpen] = useState(false);
  const welcome = !settings.welcomeDone;
  const dialogOpen = hotkeysOpen || templatesOpen || cableOpen || clearOpen || welcome;
  const mapRef = useRef<HTMLDivElement>(null);
  const bindings = useMemo(() => mergeBindings(settings.hotkeys), [settings.hotkeys]);

  // Причина ошибки из Rust → понятный текст.
  const reason = useCallback(
    (e: unknown) => {
      const code = isCmdError(e) ? e.code : 'other';
      return t(DICTS.en[`reason.${code}`] ? `reason.${code}` : 'reason.other');
    },
    [t],
  );

  // Автосохранение: только когда проект действительно изменился (выделение и размеры не в счёт).
  const projectJson = useMemo(() => JSON.stringify(fromFlow(nodes, edges)), [nodes, edges]);
  const lastSaved = useRef(JSON.stringify(project));
  useEffect(() => {
    if (projectJson === lastSaved.current) return;
    setSaveState('saving');
    const h = setTimeout(() => {
      api
        .saveProject(JSON.parse(projectJson) as Project)
        .then(() => {
          lastSaved.current = projectJson;
          setSaveState('saved');
        })
        .catch((e: unknown) => {
          setSaveState('error');
          notify('error.save', { reason: reason(e) }, true);
        });
    }, SAVE_DELAY_MS);
    return () => clearTimeout(h);
  }, [projectJson, notify, reason]);

  const titleOf = useCallback(
    (id: string) => {
      const n = nodes.find((x) => x.id === id);
      return n ? t(`node.${n.data.kind}.title`) : id;
    },
    [nodes, t],
  );
  // Подпись провода «откуда → куда» видна при наведении (см. app.css).
  const shownEdges = useMemo(
    () => edges.map((e) => ({ ...e, label: `${titleOf(e.source)} → ${titleOf(e.target)}` })),
    [edges, titleOf],
  );

  const onConnect = useCallback(
    (c: Connection) => {
      const check = canConnect(nodes, edges, c.source, c.target);
      if (check === 'cycle') notify('notice.cycle');
      if (check === 'ok') setEdges((es) => addEdge({ ...c, id: edgeId(c.source, c.target) }, es));
    },
    [nodes, edges, setEdges, notify],
  );

  const isValidConnection = useCallback(
    (c: Connection | Edge) => canConnect(nodes, edges, c.source, c.target) !== 'invalid',
    [nodes, edges],
  );

  const onSelectionChange = useCallback(({ nodes: ns, edges: es }: OnSelectionChangeParams) => {
    setSelected({ node: ns[0]?.id, edge: ns.length ? undefined : es[0]?.id });
  }, []);

  const fitSoon = useCallback(() => {
    requestAnimationFrame(() => void rf.fitView({ padding: 0.2, duration: 300 }));
  }, [rf]);

  const addNode = useCallback(
    (kind: NodeKind) => {
      const box = mapRef.current?.getBoundingClientRect();
      const center = box
        ? rf.screenToFlowPosition({ x: box.left + box.width / 2, y: box.top + box.height / 2 })
        : { x: 0, y: 0 };
      const shift = (nodes.length % 5) * 24;
      const node = newNode(kind, nextNodeId(nodes), center.x - NODE_WIDTH / 2 + shift, center.y - NODE_HEIGHT / 2 + shift);
      setNodes((ns) => [...ns.map((n) => ({ ...n, selected: false })), { ...node, selected: true }]);
    },
    [nodes, rf, setNodes],
  );

  const onParams = useCallback(
    (id: string, patch: Params) =>
      setNodes((ns) =>
        ns.map((n) => (n.id === id ? { ...n, data: { ...n.data, params: { ...n.data.params, ...patch } } } : n)),
      ),
    [setNodes],
  );

  const deleteNode = useCallback(
    (id: string) => {
      setNodes((ns) => ns.filter((n) => n.id !== id));
      setEdges((es) => es.filter((e) => e.source !== id && e.target !== id));
      setSelected({});
    },
    [setNodes, setEdges],
  );

  const deleteEdge = useCallback(
    (id: string) => {
      setEdges((es) => es.filter((e) => e.id !== id));
      setSelected({});
    },
    [setEdges],
  );

  const onArrange = useCallback(() => {
    setNodes((ns) => arrange(ns, edges));
    fitSoon();
  }, [edges, setNodes, fitSoon]);

  const addTemplate = useCallback(
    (tpl: Template) => {
      const r = applyTemplate(nodes, edges, tpl);
      setNodes(r.nodes);
      setEdges(r.edges);
      setSelected({});
      setTemplatesOpen(false);
      fitSoon();
      notify('notice.templateAdded', { name: t(`template.${tpl.id}.title`) });
    },
    [nodes, edges, setNodes, setEdges, fitSoon, notify, t],
  );

  // Приветствие показывается один раз: любой выбор в нём отмечает, что человек его видел.
  const closeWelcome = useCallback(() => onSettings({ ...settings, welcomeDone: true }), [onSettings, settings]);

  const clearAll = useCallback(() => {
    setNodes([]);
    setEdges([]);
    setSelected({});
    setClearOpen(false);
    notify('notice.cleared');
  }, [setNodes, setEdges, notify]);

  const projectFilter = useMemo(() => [{ name: t('dialog.projectFilter'), extensions: ['json'] }], [t]);

  const onExport = useCallback(async () => {
    const path = await save({ defaultPath: 'soundbimba-project.json', filters: projectFilter });
    if (!path) return;
    try {
      await api.exportProject(path, fromFlow(nodes, edges));
      notify('notice.exported');
    } catch (e) {
      notify('error.export', { reason: reason(e) }, true);
    }
  }, [nodes, edges, projectFilter, notify, reason]);

  const onImport = useCallback(async () => {
    const path = await open({ multiple: false, directory: false, filters: projectFilter });
    if (typeof path !== 'string') return;
    try {
      const flow = toFlow(await api.importProject(path));
      setNodes(flow.nodes);
      setEdges(flow.edges);
      setSelected({});
      fitSoon();
      notify('notice.imported');
    } catch (e) {
      notify('error.import', { reason: reason(e) }, true);
    }
  }, [projectFilter, setNodes, setEdges, fitSoon, notify, reason]);

  const onDiagnostics = useCallback(async () => {
    const day = new Date().toISOString().slice(0, 10);
    const path = await save({
      defaultPath: `soundbimba-diagnostics-${day}.zip`,
      filters: [{ name: t('dialog.zipFilter'), extensions: ['zip'] }],
    });
    if (!path) return;
    const uiInfo = JSON.stringify({
      version,
      language: lang,
      languageSetting: settings.language,
      theme: settings.theme,
      nodes: nodes.length,
      edges: edges.length,
      screen: `${window.screen.width}x${window.screen.height}@${window.devicePixelRatio}`,
    });
    try {
      await api.saveDiagnostics(path, uiInfo);
      notify('notice.diagnosticsSaved');
    } catch (e) {
      notify('error.diagnostics', { reason: reason(e) }, true);
    }
  }, [t, version, lang, settings, nodes.length, edges.length, notify, reason]);

  // Горячие клавиши — только пока окно SoundBimba активно и человек не печатает в поле ввода.
  const commands = useRef<Record<Action, () => void>>(null as unknown as Record<Action, () => void>);
  commands.current = {
    ...(Object.fromEntries(
      ACTIONS.flatMap((a) => {
        const kind = addedKind(a);
        return kind ? [[a, () => addNode(kind)]] : [];
      }),
    ) as Record<Action, () => void>),
    deleteSelected: () => {
      if (selected.node) deleteNode(selected.node);
      else if (selected.edge) deleteEdge(selected.edge);
    },
    deselect: () => {
      setNodes((ns) => ns.map((n) => (n.selected ? { ...n, selected: false } : n)));
      setEdges((es) => es.map((e) => (e.selected ? { ...e, selected: false } : e)));
      setSelected({});
    },
    arrange: onArrange,
    fitView: fitSoon,
    export: () => void onExport(),
    import: () => void onImport(),
    diagnostics: () => void onDiagnostics(),
  };
  useEffect(() => {
    if (dialogOpen) return; // пока открыто окно (в «Клавишах» нажатия идут на переназначение)
    const onKey = (e: KeyboardEvent) => {
      if (e.repeat || isTyping(e.target)) return;
      const combo = eventToCombo(e);
      const action = combo ? actionFor(bindings, combo) : undefined;
      if (!action) return;
      e.preventDefault();
      commands.current[action]();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [bindings, dialogOpen]);

  const changeBindings = useCallback(
    (b: Bindings) => onSettings({ ...settings, hotkeys: b }),
    [onSettings, settings],
  );

  const [measuring, setMeasuring] = useState(false);
  const onLatency = useCallback(async () => {
    if (measuring) return;
    setMeasuring(true);
    notify('notice.latencyMeasuring');
    try {
      const l = await api.measureLatency();
      notify('notice.latency', { output: Math.round(l.outputMs), total: Math.round(l.outputMs + l.inputEstimateMs) });
    } catch (e) {
      const code = isCmdError(e) ? e.code : 'other';
      notify(code === 'no-cable' ? 'error.latencyNoCable' : 'error.latency', undefined, true);
    } finally {
      setMeasuring(false);
    }
  }, [measuring, notify]);

  // Кнопочная панель: отдельное окно поверх остальных; если уже открыто — показать его.
  const openPanel = useCallback(async () => {
    if (!('__TAURI_INTERNALS__' in window)) {
      window.open('/#buttons', 'buttons', 'width=440,height=340');
      return;
    }
    const existing = await WebviewWindow.getByLabel('buttons');
    if (existing) {
      await existing.setFocus();
      return;
    }
    const w = new WebviewWindow('buttons', {
      url: 'index.html#buttons',
      title: t('buttons.title'),
      width: 440,
      height: 340,
      minWidth: 220,
      minHeight: 160,
      alwaysOnTop: true,
      decorations: settings.panelFrame !== false,
    });
    void w.once('tauri://error', (e) => api.log('error', `button panel: ${JSON.stringify(e.payload)}`));
  }, [t, settings.panelFrame]);

  const ariaLabelConfig = useMemo(
    () => ({
      'node.a11yDescription.default': t('map.nodeDesc'),
      'node.a11yDescription.keyboardDisabled': t('map.nodeDesc'),
      'edge.a11yDescription.default': t('map.edgeDesc'),
      'controls.ariaLabel': t('map.controls'),
      'controls.zoomIn.ariaLabel': t('map.zoomIn'),
      'controls.zoomOut.ariaLabel': t('map.zoomOut'),
      'controls.fitView.ariaLabel': t('map.fitView'),
      'controls.interactive.ariaLabel': t('map.interactive'),
      'minimap.ariaLabel': t('map.minimap'),
      'handle.ariaLabel': t('map.handle'),
    }),
    [t],
  );

  const selectedNode = nodes.find((n) => n.id === selected.node);
  const selectedEdge = shownEdges.find((e) => e.id === selected.edge);

  return (
    <div className="workspace">
      <Toolbar
        lang={lang}
        settings={settings}
        saveState={saveState}
        bindings={bindings}
        onToggleAudio={onToggleAudio}
        onAdd={addNode}
        onArrange={onArrange}
        onExport={() => void onExport()}
        onImport={() => void onImport()}
        onDiagnostics={() => void onDiagnostics()}
        onLatency={() => void onLatency()}
        onHotkeys={() => setHotkeysOpen(true)}
        onTemplates={() => setTemplatesOpen(true)}
        onCable={() => setCableOpen(true)}
        onClear={() => setClearOpen(true)}
        onSettings={onSettings}
        onOpenPanel={() => void openPanel()}
        simple={settings.mode === 'simple'}
      />
      {settings.mode === 'simple' ? (
        <main className="main-simple">
          <SimpleView project={JSON.parse(projectJson) as Project} onParams={onParams} />
        </main>
      ) : (
      <main className="main">
        <div className="map" ref={mapRef}>
          <Watermark />
          {nodes.length === 0 && (
            <div className="empty-hint">
              <strong>{t('empty.title')}</strong>
              <span className="panel-desc">{t('empty.hint')}</span>
              <button type="button" className="primary" onClick={() => setTemplatesOpen(true)}>
                {t('welcome.templates')}
              </button>
            </div>
          )}
          <ReactFlow
            nodes={nodes}
            edges={shownEdges}
            nodeTypes={nodeTypes}
            onNodesChange={onNodesChange}
            onEdgesChange={onEdgesChange}
            onConnect={onConnect}
            isValidConnection={isValidConnection}
            onSelectionChange={onSelectionChange}
            fitView
            fitViewOptions={{ padding: 0.2 }}
            snapToGrid
            snapGrid={[20, 20]}
            colorMode="dark"
            /* Встроенные клавиши React Flow выключены: свои клавиши (Delete и др.) — в hotkeys.ts,
               их можно переназначить. */
            deleteKeyCode={null}
            selectionKeyCode={null}
            multiSelectionKeyCode={null}
            zoomActivationKeyCode={null}
            panActivationKeyCode={null}
            disableKeyboardA11y
            ariaLabelConfig={ariaLabelConfig}
            proOptions={{ hideAttribution: true }}
          >
            <Background variant={BackgroundVariant.Dots} gap={20} size={1.5} />
            <MiniMap pannable zoomable nodeClassName={(n) => `cat-${KINDS[(n as SoundNode).data.kind].category}`} />
            <Controls showInteractive={false} />
          </ReactFlow>
        </div>
        <SidePanel
          node={selectedNode}
          edge={selectedEdge}
          edgeLabel={typeof selectedEdge?.label === 'string' ? selectedEdge.label : undefined}
          onParams={onParams}
          onDeleteNode={deleteNode}
          onDeleteEdge={deleteEdge}
        />
      </main>
      )}
      {templatesOpen && <TemplatesDialog onPick={addTemplate} onClose={() => setTemplatesOpen(false)} />}
      {cableOpen && <CableWizard onClose={() => setCableOpen(false)} />}
      {clearOpen && (
        <div className="dialog-backdrop">
          <div className="dialog dialog-narrow" role="alertdialog" aria-modal="true" aria-labelledby="clear-title">
            <h2 id="clear-title" className="panel-title">
              {t('clear.title')}
            </h2>
            <p className="panel-desc">{t('clear.text')}</p>
            <div className="dialog-actions">
              <span className="toolbar-gap" />
              <button type="button" autoFocus onClick={() => setClearOpen(false)}>
                {t('clear.no')}
              </button>
              <button type="button" className="danger" onClick={clearAll}>
                {t('clear.yes')}
              </button>
            </div>
          </div>
        </div>
      )}
      {welcome && !templatesOpen && !cableOpen && (
        <WelcomeDialog
          onTemplates={() => {
            closeWelcome();
            setTemplatesOpen(true);
          }}
          onCable={() => {
            closeWelcome();
            setCableOpen(true);
          }}
          onClose={closeWelcome}
        />
      )}
      {hotkeysOpen && <HotkeysDialog bindings={bindings} onChange={changeBindings} onClose={() => setHotkeysOpen(false)} />}
    </div>
  );
}
