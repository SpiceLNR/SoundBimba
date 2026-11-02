// SPDX-License-Identifier: AGPL-3.0-or-later
// Модель карты на стороне интерфейса: та же схема, что в crates/graph (Rust проверяет её при сохранении).
import type { Edge, Node } from '@xyflow/react';
import { signed } from './compressor';
import { voiceOf } from './morpher';
import type { Translate } from './i18n';

export type NodeKind = 'app-source' | 'mic-input' | 'sound-button' | 'volume' | 'compressor' | 'voice-morpher' | 'speaker-out' | 'virtual-out';
export type Category = 'source' | 'processing' | 'output' | 'button';
export type Params = Record<string, unknown>;

interface KindInfo {
  category: Category;
  input: boolean;
  output: boolean;
  defaults: Params;
}

export const KINDS: Record<NodeKind, KindInfo> = {
  'app-source': { category: 'source', input: false, output: true, defaults: { exe: '' } },
  'mic-input': { category: 'source', input: false, output: true, defaults: { device: 'default' } },
  'sound-button': { category: 'button', input: false, output: true, defaults: { file: '' } },
  volume: { category: 'processing', input: true, output: true, defaults: { gain: 1, mute: false } },
  // Те же значения, что набор «Голос» и COMPRESSOR_DEFAULTS в движке.
  compressor: {
    category: 'processing',
    input: true,
    output: true,
    defaults: { threshold: -24, ratio: 3, attack: 5, release: 120, makeup: 12 },
  },
  // Голос «Обычный»: все эффекты выключены, голос проходит без изменений.
  'voice-morpher': { category: 'processing', input: true, output: true, defaults: { pitch: 0, robot: 0, radio: 0, echo: 0, wobble: 0 } },
  'speaker-out': { category: 'output', input: true, output: false, defaults: { device: 'default' } },
  'virtual-out': { category: 'output', input: true, output: false, defaults: { device: 'virtual-1' } },
};

export const KIND_ORDER = Object.keys(KINDS) as NodeKind[];
export const CATEGORY_ORDER: Category[] = ['source', 'button', 'processing', 'output'];

export interface ProjectNode {
  id: string;
  type: NodeKind;
  pos: [number, number];
  params: Params;
}

export interface ProjectEdge {
  from: string;
  to: string;
  kind?: 'audio' | 'control';
}

export interface Project {
  schema: number;
  nodes: ProjectNode[];
  edges: ProjectEdge[];
}

export const SCHEMA = 1;

export interface SoundData extends Record<string, unknown> {
  kind: NodeKind;
  params: Params;
}

export type SoundNode = Node<SoundData, 'sound'>;

export const NODE_WIDTH = 200;
export const NODE_HEIGHT = 64;

export function edgeId(from: string, to: string): string {
  return `${from}->${to}`;
}

export function toFlow(p: Project): { nodes: SoundNode[]; edges: Edge[] } {
  return {
    nodes: p.nodes.map((n) => ({
      id: n.id,
      type: 'sound',
      position: { x: n.pos[0], y: n.pos[1] },
      data: { kind: n.type, params: { ...n.params } },
    })),
    edges: p.edges.map((e) => ({
      id: edgeId(e.from, e.to),
      source: e.from,
      target: e.to,
      data: e.kind && e.kind !== 'audio' ? { kind: e.kind } : undefined,
    })),
  };
}

const round = (v: number) => Math.round(v * 10) / 10;

export function fromFlow(nodes: SoundNode[], edges: Edge[]): Project {
  return {
    schema: SCHEMA,
    nodes: nodes.map((n) => ({
      id: n.id,
      type: n.data.kind,
      pos: [round(n.position.x), round(n.position.y)],
      params: n.data.params,
    })),
    edges: edges.map((e) => {
      const kind = (e.data as { kind?: 'control' } | undefined)?.kind;
      return kind ? { from: e.source, to: e.target, kind } : { from: e.source, to: e.target };
    }),
  };
}

/** Есть ли уже путь to → … → from (тогда провод from → to замкнёт круг). */
export function wouldCreateCycle(edges: Pick<Edge, 'source' | 'target'>[], from: string, to: string): boolean {
  if (from === to) return true;
  const stack = [to];
  const seen = new Set<string>();
  while (stack.length > 0) {
    const id = stack.pop() as string;
    if (id === from) return true;
    if (seen.has(id)) continue;
    seen.add(id);
    for (const e of edges) if (e.source === id) stack.push(e.target);
  }
  return false;
}

export type ConnectCheck = 'ok' | 'cycle' | 'invalid';

export function canConnect(nodes: SoundNode[], edges: Edge[], from: string, to: string): ConnectCheck {
  const a = nodes.find((n) => n.id === from);
  const b = nodes.find((n) => n.id === to);
  if (!a || !b || !KINDS[a.data.kind].output || !KINDS[b.data.kind].input) return 'invalid';
  if (edges.some((e) => e.source === from && e.target === to)) return 'invalid';
  return wouldCreateCycle(edges, from, to) ? 'cycle' : 'ok';
}

export function nextNodeId(nodes: Pick<SoundNode, 'id'>[]): string {
  const max = nodes.reduce((m, n) => Math.max(m, Number(/^n(\d+)$/.exec(n.id)?.[1] ?? 0)), 0);
  return `n${max + 1}`;
}

export function newNode(kind: NodeKind, id: string, x: number, y: number): SoundNode {
  return { id, type: 'sound', position: { x, y }, data: { kind, params: { ...KINDS[kind].defaults } } };
}

const COLUMN_STEP = NODE_WIDTH + 100;
const ROW_STEP = NODE_HEIGHT + 46;
const LOOSE_GAP = 70;

/**
 * Автораскладка «Упорядочить» — столбцами слева направо:
 *  - первый столбец — источники и кнопки, последний — выходы, между ними — обработка
 *    (громкость после громкости уходит на столбец правее);
 *  - внутри столбца узлы стоят так, чтобы провода меньше пересекались;
 *  - узлы без проводов не сбиваются в одну линию: каждый встаёт в столбец своего типа,
 *    ниже соединённой схемы — как «ещё не подключённые».
 */
export function arrange(nodes: SoundNode[], edges: Edge[]): SoundNode[] {
  const ids = new Set(nodes.map((n) => n.id));
  const links = edges.filter((e) => ids.has(e.source) && ids.has(e.target));
  const preds = new Map<string, string[]>(nodes.map((n) => [n.id, []]));
  const succs = new Map<string, string[]>(nodes.map((n) => [n.id, []]));
  for (const e of links) {
    preds.get(e.target)!.push(e.source);
    succs.get(e.source)!.push(e.target);
  }
  const kindOf = new Map(nodes.map((n) => [n.id, n.data.kind]));
  const cat = (id: string) => KINDS[kindOf.get(id)!].category;

  // Столбец обработки = 1 + самый дальний столбец среди тех, кто подаёт в него звук.
  const procRank = new Map<string, number>();
  const rankOf = (id: string, seen = new Set<string>()): number => {
    if (cat(id) === 'source' || cat(id) === 'button') return 0;
    const known = procRank.get(id);
    if (known !== undefined) return known;
    if (seen.has(id)) return 1; // циклов быть не должно; защита от зависания
    seen.add(id);
    const r = 1 + Math.max(0, ...preds.get(id)!.map((p) => (cat(p) === 'output' ? 0 : rankOf(p, seen))));
    procRank.set(id, r);
    return r;
  };
  const processing = nodes.filter((n) => cat(n.id) === 'processing');
  const maxProc = Math.max(0, ...processing.map((n) => rankOf(n.id)));
  const last = processing.length > 0 ? Math.max(maxProc, 1) + 1 : 1;
  const column = (id: string) => (cat(id) === 'output' ? last : rankOf(id));

  const isLinked = (id: string) => preds.get(id)!.length > 0 || succs.get(id)!.length > 0;
  const byY = (a: SoundNode, b: SoundNode) => a.position.y - b.position.y || a.id.localeCompare(b.id);
  const linked = nodes.filter((n) => isLinked(n.id)).sort(byY);
  const loose = nodes.filter((n) => !isLinked(n.id)).sort(byY);

  // Соединённая часть: по столбцам, затем порядок внутри них по «центру тяжести» соседей.
  const cols: string[][] = Array.from({ length: last + 1 }, () => []);
  for (const n of linked) cols[column(n.id)].push(n.id);
  const index = new Map<string, number>();
  const reindex = () => cols.forEach((c) => c.forEach((id, i) => index.set(id, i)));
  reindex();
  const sortBy = (col: string[], neighbours: Map<string, string[]>) => {
    const centre = (id: string) => {
      const ns = neighbours.get(id)!.filter((x) => index.has(x));
      return ns.length ? ns.reduce((sum, x) => sum + index.get(x)!, 0) / ns.length : index.get(id)!;
    };
    const keyed = col.map((id) => ({ id, key: centre(id) }));
    keyed.sort((a, b) => a.key - b.key || index.get(a.id)! - index.get(b.id)!);
    col.splice(0, col.length, ...keyed.map((k) => k.id));
  };
  for (let pass = 0; pass < 4; pass++) {
    for (let c = 1; c <= last; c++) {
      sortBy(cols[c], preds);
      reindex();
    }
    for (let c = last - 1; c >= 0; c--) {
      sortBy(cols[c], succs);
      reindex();
    }
  }

  const pos = new Map<string, { x: number; y: number }>();
  const tallest = Math.max(0, ...cols.map((c) => c.length));
  cols.forEach((col, c) => {
    const top = ((tallest - col.length) * ROW_STEP) / 2; // столбцы выровнены по центру
    col.forEach((id, i) => pos.set(id, { x: c * COLUMN_STEP, y: top + i * ROW_STEP }));
  });

  // Неподключённые — ниже схемы, каждый в столбце своего типа.
  const looseTop = tallest > 0 ? tallest * ROW_STEP + LOOSE_GAP : 0;
  const looseCount = new Array<number>(last + 1).fill(0);
  for (const n of loose) {
    const c = column(n.id);
    pos.set(n.id, { x: c * COLUMN_STEP, y: looseTop + looseCount[c]++ * ROW_STEP });
  }

  return nodes.map((n) => ({ ...n, position: pos.get(n.id)! }));
}

/** Короткая подпись под названием узла: что выбрано. */
export function summary(kind: NodeKind, params: Params, t: Translate): string {
  switch (kind) {
    case 'app-source':
      return typeof params.exe === 'string' && params.exe ? params.exe : t('param.exeEmpty');
    case 'sound-button':
      return typeof params.file === 'string' && params.file ? params.file : t('param.fileEmpty');
    case 'volume': {
      const gain = `${Math.round(Number(params.gain ?? 1) * 100)} %`;
      return params.mute === true ? `${gain} · ${t('param.muted')}` : gain;
    }
    case 'compressor':
      return `${signed(Number(params.threshold ?? -24))} ${t('unit.db')} · ${Number(params.ratio ?? 3)}:1 · +${Number(params.makeup ?? 12)} ${t('unit.db')}`;
    case 'voice-morpher': {
      const voice = voiceOf(params);
      return voice ? t(`voice.${voice}`) : t('voice.custom');
    }
    case 'mic-input':
    case 'speaker-out':
      return params.device === 'default' || params.device === undefined ? t('param.deviceDefault') : String(params.device);
    case 'virtual-out':
      return '';
  }
}

/** Как узел называется в простом режиме: для программы — имя exe, для кнопки — её подпись. */
export function shortName(n: ProjectNode, t: Translate): string {
  if (n.type === 'app-source' && typeof n.params.exe === 'string' && n.params.exe) return n.params.exe;
  if (n.type === 'sound-button') {
    if (typeof n.params.label === 'string' && n.params.label.trim()) return n.params.label.trim();
    if (typeof n.params.file === 'string' && n.params.file) {
      return (n.params.file.split(/[\\/]/).pop() ?? n.params.file).replace(/\.[^.]+$/, '');
    }
  }
  return t(`node.${n.type}.title`);
}

export interface VolumeRoute {
  id: string;
  /** Что подаёт звук в эту громкость, через запятую. */
  from: string;
  /** Куда звук уходит дальше, через запятую. */
  to: string;
}

/** Регуляторы для простого режима (громкость, морфер): «откуда → куда» по соседним узлам. */
export function volumeRoutes(p: Project, t: Translate, kind: NodeKind = 'volume'): VolumeRoute[] {
  const byId = new Map(p.nodes.map((n) => [n.id, n]));
  const names = (ids: string[]) =>
    [...new Set(ids.map((id) => byId.get(id)).filter((n): n is ProjectNode => !!n).map((n) => shortName(n, t)))].join(', ');
  return p.nodes
    .filter((n) => n.type === kind)
    .map((n) => ({
      id: n.id,
      from: names(p.edges.filter((e) => e.to === n.id).map((e) => e.from)),
      to: names(p.edges.filter((e) => e.from === n.id).map((e) => e.to)),
    }));
}
