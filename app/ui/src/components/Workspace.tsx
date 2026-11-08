// SPDX-License-Identifier: AGPL-3.0-or-later
// Рабочая среда: панель инструментов, карта (React Flow) и боковая панель. Автосохранение проекта.
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { open, save } from '@tauri-apps/plugin-dialog';
import {
  addEdge,
  Background,
  BackgroundVariant,
  ConnectionLineType,
  Controls,
  MiniMap,
  ReactFlow,
  useEdgesState,
  useNodesState,
  useReactFlow,
  useUpdateNodeInternals,
  type Connection,
  type Edge,
  type OnSelectionChangeParams,
} from '@xyflow/react';
import { type MouseEvent, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { api, isCmdError, onTemplatesOpened, type Settings, type UserTemplate } from '../api';
import {
  arrange,
  canConnect,
  edgeId,
  chipInner,
  composeRoot,
  innerFromFlow,
  lookOf,
  withInner,
  flattenProject,
  patchNested,
  type ChipLevel,
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
import { wireShape } from '../theme';
import { applyTemplate, type Template } from '../templates';
import { IdPrefix } from './chipContext';
import TraceEdge from './TraceEdge';
import CableWizard from './CableWizard';
import HotkeysDialog from './HotkeysDialog';
import type { Notify } from './Notices';
import SidePanel from './SidePanel';
import SimpleView from './SimpleView';
import SoundNodeView from './SoundNodeView';
import TemplatesDialog from './TemplatesDialog';
import Toolbar, { type SaveState } from './Toolbar';
import Mascot from './Mascot';
import Watermark from './Watermark';
import WelcomeDialog from './WelcomeDialog';

const nodeTypes = { sound: SoundNodeView };
const edgeTypes = { trace: TraceEdge };
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
  // Открытые микросхемы: карта под каждой (на экране — схема внутри последней).
  const [stack, setStack] = useState<ChipLevel[]>([]);
  const inChip = stack.length > 0;
  const [saveState, setSaveState] = useState<SaveState>('saved');
  const [hotkeysOpen, setHotkeysOpen] = useState(false);
  const [templatesOpen, setTemplatesOpen] = useState(false);
  const [freshTemplates, setFreshTemplates] = useState<string[]>([]);
  const [cableOpen, setCableOpen] = useState(false);
  const [clearOpen, setClearOpen] = useState(false);
  // Подпись провода при наведении: обычный слой поверх карты (подпись React Flow рисуется в
  // слое проводов, то есть под узлами).
  const [wireTip, setWireTip] = useState<{ text: string; x: number; y: number } | null>(null);
  const showWireTip = useCallback((ev: MouseEvent, edge: Edge) => {
    const box = mapRef.current?.getBoundingClientRect();
    if (!box || typeof edge.label !== 'string') return;
    setWireTip({ text: edge.label, x: ev.clientX - box.left, y: ev.clientY - box.top });
  }, []);
  const hideWireTip = useCallback(() => setWireTip(null), []);
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
  const projectJson = useMemo(() => JSON.stringify(composeRoot(stack, nodes, edges)), [stack, nodes, edges]);
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
  // Форма проводов — из темы: у «Инженерской» дорожки с прямыми углами, как на плате.
  const traces = wireShape(settings.theme) === 'traces';
  const edgeType = traces ? 'trace' : 'default';
  // Подпись провода «откуда → куда» видна при наведении (см. app.css).
  const shownEdges = useMemo(
    () => edges.map((e) => ({ ...e, type: edgeType, label: `${titleOf(e.source)} → ${titleOf(e.target)}` })),
    [edges, titleOf, edgeType],
  );

  const onConnect = useCallback(
    (c: Connection) => {
      const check = canConnect(nodes, edges, c.source, c.target, c.sourceHandle, c.targetHandle);
      if (check === 'cycle') notify('notice.cycle');
      if (check === 'ok') setEdges((es) => addEdge({ ...c, id: edgeId(c.source, c.target, c.sourceHandle, c.targetHandle) }, es));
    },
    [nodes, edges, setEdges, notify],
  );

  const isValidConnection = useCallback(
    (c: Connection | Edge) => canConnect(nodes, edges, c.source, c.target, c.sourceHandle, c.targetHandle) !== 'invalid',
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
      if (KINDS[kind].insideChip && !inChip) return; // ножки — только внутри микросхемы
      const box = mapRef.current?.getBoundingClientRect();
      const center = box
        ? rf.screenToFlowPosition({ x: box.left + box.width / 2, y: box.top + box.height / 2 })
        : { x: 0, y: 0 };
      const shift = (nodes.length % 5) * 24;
      const node = newNode(kind, nextNodeId(nodes), center.x - NODE_WIDTH / 2 + shift, center.y - NODE_HEIGHT / 2 + shift);
      setNodes((ns) => [...ns.map((n) => ({ ...n, selected: false })), { ...node, selected: true }]);
    },
    [nodes, rf, setNodes, inChip],
  );

  // Открыть микросхему: карта уходит «под» неё, на экране — схема внутри.
  const enterChip = useCallback(
    (chipId: string) => {
      const chip = nodes.find((n) => n.id === chipId);
      if (!chip || chip.data.kind !== 'chip') return;
      const inner = toFlow({ schema: 1, ...chipInner(chip.data.params) });
      setStack((st) => [...st, { chipId, nodes: nodes.map((n) => ({ ...n, selected: false })), edges }]);
      setNodes(inner.nodes);
      setEdges(inner.edges);
      setSelected({});
      fitSoon();
    },
    [nodes, edges, setNodes, setEdges, fitSoon],
  );

  // Вернуться на уровень `depth` (0 — карта): схема на экране складывается обратно в микросхемы.
  const exitTo = useCallback(
    (depth: number) => {
      if (depth >= stack.length) return;
      let ns = nodes;
      let es = edges;
      for (let i = stack.length - 1; i >= depth; i--) {
        const level = stack[i];
        ns = withInner(level.nodes, level.chipId, innerFromFlow(ns, es));
        es = level.edges;
      }
      setStack(stack.slice(0, depth));
      setNodes(ns);
      setEdges(es);
      setSelected({});
      fitSoon();
    },
    [stack, nodes, edges, setNodes, setEdges, fitSoon],
  );

  // У узлов разных уровней одинаковые номера (n1, n2…), а карта помнит точки подключения по
  // номеру — после входа в микросхему и выхода из неё просим перемерить узлы, иначе провода к
  // «чужим» точкам не рисуются (в проекте они при этом есть).
  const updateNodeInternals = useUpdateNodeInternals();
  const levelKey = stack.map((l) => l.chipId).join('/');
  const nodesNow = useRef(nodes);
  nodesNow.current = nodes;
  useEffect(() => {
    const h = requestAnimationFrame(() => updateNodeInternals(nodesNow.current.map((n) => n.id)));
    return () => cancelAnimationFrame(h);
  }, [levelKey, updateNodeInternals]);

  // В простом режиме — весь проект: открытые микросхемы закрываем.
  useEffect(() => {
    if (settings.mode === 'simple' && stack.length) exitTo(0);
  }, [settings.mode, stack.length, exitTo]);

  // id узла на открытом уровне — или полный («микросхема/…/узел») из простого режима.
  const onParams = useCallback(
    (id: string, patch: Params) => {
      const [head, ...rest] = id.split('/');
      setNodes((ns) =>
        ns.map((n) =>
          n.id !== head
            ? n
            : { ...n, data: { ...n.data, params: rest.length ? patchNested(n.data.params, rest, patch) : { ...n.data.params, ...patch } } },
        ),
      );
    },
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
    (tpl: Template, name: string) => {
      const r = applyTemplate(nodes, edges, tpl);
      setNodes(r.nodes);
      setEdges(r.edges);
      setSelected({});
      setTemplatesOpen(false);
      fitSoon();
      notify('notice.templateAdded', { name });
    },
    [nodes, edges, setNodes, setEdges, fitSoon, notify],
  );

  // Файлы .bmb, открытые двойным щелчком: они уже в «Моих шаблонах» — показываем окно шаблонов.
  const showOpened = useCallback(
    (added: UserTemplate[], errors: string[]) => {
      for (const a of added) notify('notice.templateImported', { name: a.name });
      for (const code of errors) notify('error.templateOpen', { reason: reason({ code, detail: '' }) }, true);
      if (added.length) {
        setFreshTemplates(added.map((a) => a.id));
        setTemplatesOpen(true);
      }
    },
    [notify, reason],
  );
  useEffect(() => {
    void api.takeOpenedTemplates().then((o) => o && showOpened(o.templates, o.errors));
    return onTemplatesOpened((o) => showOpened(o.templates, o.errors));
  }, [showOpened]);

  // Приветствие показывается один раз: любой выбор в нём отмечает, что человек его видел.
  const closeWelcome = useCallback(() => onSettings({ ...settings, welcomeDone: true }), [onSettings, settings]);

  const clearAll = useCallback(() => {
    setStack([]);
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
      setStack([]);
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
        inChip={inChip}
      />
      {settings.mode === 'simple' ? (
        <main className="main-simple">
          <SimpleView project={flattenProject(JSON.parse(projectJson) as Project)} onParams={onParams} />
        </main>
      ) : (
      <main className="main">
        <div className="map" ref={mapRef}>
          <Watermark />
          <Mascot theme={settings.theme} />
          {inChip && (
            <nav className="chip-path" aria-label={t('chip.path')}>
              <button type="button" className="chip-back" onClick={() => exitTo(stack.length - 1)}>
                ← {t('chip.back')}
              </button>
              <button type="button" className="link-button" onClick={() => exitTo(0)}>
                {t('chip.map')}
              </button>
              {stack.map((level, i) => {
                const chip = level.nodes.find((n) => n.id === level.chipId);
                const name = typeof chip?.data.params.name === 'string' && chip.data.params.name.trim() ? chip.data.params.name.trim() : t('node.chip.title');
                const last = i === stack.length - 1;
                return (
                  <span key={i} className="chip-path-step">
                    <span aria-hidden="true">›</span>
                    {last ? (
                      <strong aria-current="page">{name}</strong>
                    ) : (
                      <button type="button" className="link-button" onClick={() => exitTo(i + 1)}>
                        {name}
                      </button>
                    )}
                  </span>
                );
              })}
            </nav>
          )}
          {wireTip && (
            <div className="wire-tip" style={{ left: wireTip.x, top: wireTip.y }} role="tooltip">
              {wireTip.text}
            </div>
          )}
          {nodes.length === 0 && (
            <div className="empty-hint">
              <strong>{t('empty.title')}</strong>
              <span className="panel-desc">{t('empty.hint')}</span>
              <button type="button" className="primary" onClick={() => setTemplatesOpen(true)}>
                {t('welcome.templates')}
              </button>
            </div>
          )}
          <IdPrefix.Provider value={stack.map((l) => `${l.chipId}/`).join('')}>
          <ReactFlow
            nodes={nodes}
            edges={shownEdges}
            nodeTypes={nodeTypes}
            edgeTypes={edgeTypes}
            onNodesChange={onNodesChange}
            onEdgesChange={onEdgesChange}
            onConnect={onConnect}
            isValidConnection={isValidConnection}
            onSelectionChange={onSelectionChange}
            onNodeDoubleClick={(_, n) => enterChip(n.id)}
            onEdgeMouseEnter={showWireTip}
            onEdgeMouseMove={showWireTip}
            onEdgeMouseLeave={hideWireTip}
            fitView
            fitViewOptions={{ padding: 0.2 }}
            connectionLineType={traces ? ConnectionLineType.SmoothStep : ConnectionLineType.Bezier}
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
            <MiniMap pannable zoomable nodeClassName={(n) => `cat-${lookOf((n as SoundNode).data.kind)}`} />
            <Controls showInteractive={false} />
          </ReactFlow>
          </IdPrefix.Provider>
        </div>
        <IdPrefix.Provider value={stack.map((l) => `${l.chipId}/`).join('')}>
        <SidePanel
          node={selectedNode}
          edge={selectedEdge}
          edgeLabel={typeof selectedEdge?.label === 'string' ? selectedEdge.label : undefined}
          onParams={onParams}
          onDeleteNode={deleteNode}
          onDeleteEdge={deleteEdge}
          onOpenChip={enterChip}
        />
        </IdPrefix.Provider>
      </main>
      )}
      {templatesOpen && (
        <TemplatesDialog
          project={JSON.parse(projectJson) as Project}
          fresh={freshTemplates}
          hidden={settings.hiddenTemplates ?? []}
          onHidden={(ids) => onSettings({ ...settings, hiddenTemplates: ids })}
          current={settings.currentTemplate ?? null}
          onCurrent={(id) => onSettings({ ...settings, currentTemplate: id })}
          onPick={addTemplate}
          onClose={() => {
            setTemplatesOpen(false);
            setFreshTemplates([]);
          }}
          notify={notify}
          reason={reason}
        />
      )}
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
