// SPDX-License-Identifier: AGPL-3.0-or-later
// Модель карты на стороне интерфейса: та же схема, что в crates/graph (Rust проверяет её при сохранении).
import dagre from '@dagrejs/dagre';
import type { Edge, Node } from '@xyflow/react';
import type { Translate } from './i18n';

export type NodeKind = 'app-source' | 'mic-input' | 'sound-button' | 'volume' | 'speaker-out' | 'virtual-out';
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

/** Автораскладка слева направо: источники → обработка → выходы. */
export function arrange(nodes: SoundNode[], edges: Edge[]): SoundNode[] {
  const g = new dagre.graphlib.Graph();
  g.setGraph({ rankdir: 'LR', nodesep: 40, ranksep: 90, marginx: 20, marginy: 20 });
  g.setDefaultEdgeLabel(() => ({}));
  for (const n of nodes) g.setNode(n.id, { width: NODE_WIDTH, height: NODE_HEIGHT });
  for (const e of edges) g.setEdge(e.source, e.target);
  dagre.layout(g);
  return nodes.map((n) => {
    const p = g.node(n.id);
    return { ...n, position: { x: round(p.x - NODE_WIDTH / 2), y: round(p.y - NODE_HEIGHT / 2) } };
  });
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
    case 'mic-input':
    case 'speaker-out':
      return params.device === 'default' || params.device === undefined ? t('param.deviceDefault') : String(params.device);
    case 'virtual-out':
      return '';
  }
}
