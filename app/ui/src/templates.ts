// SPDX-License-Identifier: AGPL-3.0-or-later
// Готовые схемы одним щелчком. Шаблон добавляется к карте (ниже имеющегося), ничего не стирая.
import type { Edge } from '@xyflow/react';
import { arrange, edgeId, KINDS, newNode, nextNodeId, NODE_HEIGHT, type NodeKind, type Params, type Project, type SoundNode } from './graph';

interface TemplateNode {
  /** Имя узла внутри шаблона (для проводов). */
  key: string;
  kind: NodeKind;
  params?: Params;
  /** Положение на карте (у своих шаблонов): тогда раскладка остаётся как была. */
  pos?: [number, number];
}

export interface Template {
  id: string;
  nodes: TemplateNode[];
  /** Провода: [откуда, куда] по именам узлов шаблона. */
  edges: [string, string][];
}

export const TEMPLATES: Template[] = [
  {
    id: 'soundpad-discord',
    nodes: [
      { key: 'mic', kind: 'mic-input' },
      { key: 'b1', kind: 'sound-button' },
      { key: 'b2', kind: 'sound-button' },
      { key: 'vol', kind: 'volume', params: { gain: 0.8, mute: false } },
      { key: 'virt', kind: 'virtual-out' },
      { key: 'phones', kind: 'speaker-out' },
    ],
    edges: [
      ['mic', 'virt'],
      ['b1', 'vol'],
      ['b2', 'vol'],
      ['vol', 'virt'],
      ['vol', 'phones'],
    ],
  },
  {
    id: 'game-to-mic',
    nodes: [
      { key: 'game', kind: 'app-source' },
      { key: 'mic', kind: 'mic-input' },
      { key: 'vol', kind: 'volume', params: { gain: 0.7, mute: false } },
      { key: 'virt', kind: 'virtual-out' },
    ],
    edges: [
      ['game', 'vol'],
      ['vol', 'virt'],
      ['mic', 'virt'],
    ],
  },
  {
    id: 'music-to-stream',
    nodes: [
      { key: 'player', kind: 'app-source' },
      { key: 'vol', kind: 'volume', params: { gain: 0.4, mute: false } },
      { key: 'mic', kind: 'mic-input' },
      { key: 'virt', kind: 'virtual-out' },
    ],
    edges: [
      ['player', 'vol'],
      ['vol', 'virt'],
      ['mic', 'virt'],
    ],
  },
  {
    id: 'hear-myself',
    nodes: [
      { key: 'mic', kind: 'mic-input' },
      { key: 'vol', kind: 'volume', params: { gain: 0.6, mute: false } },
      { key: 'phones', kind: 'speaker-out' },
    ],
    edges: [
      ['mic', 'vol'],
      ['vol', 'phones'],
    ],
  },
];

/** Свой шаблон (схема из файла .bmb) в том же виде, что встроенные. */
export function fromProject(id: string, p: Project): Template {
  return {
    id,
    nodes: p.nodes.map((n) => ({ key: n.id, kind: n.type, params: n.params, pos: n.pos })),
    edges: p.edges.map((e) => [e.from, e.to]),
  };
}

const GAP = 140;

/**
 * Добавляет шаблон к карте: новые узлы с новыми номерами, раскладка шаблона отдельно, ниже
 * уже имеющихся узлов. Возвращает новые списки узлов и проводов (старые не меняются).
 */
export function applyTemplate(nodes: SoundNode[], edges: Edge[], tpl: Template): { nodes: SoundNode[]; edges: Edge[]; added: string[] } {
  const ids = new Map<string, string>();
  const fresh: SoundNode[] = [];
  let all: Pick<SoundNode, 'id'>[] = [...nodes];
  for (const n of tpl.nodes) {
    const id = nextNodeId(all);
    ids.set(n.key, id);
    const node = newNode(n.kind, id, 0, 0);
    node.data.params = { ...KINDS[n.kind].defaults, ...n.params };
    fresh.push(node);
    all = [...all, node];
  }
  const freshEdges: Edge[] = tpl.edges.map(([a, b]) => {
    const source = ids.get(a)!;
    const target = ids.get(b)!;
    return { id: edgeId(source, target), source, target };
  });
  // Своя раскладка шаблона (сдвинутая в начало координат) — или автораскладка у встроенных.
  const own = tpl.nodes.length > 0 && tpl.nodes.every((n) => n.pos);
  const minX = own ? Math.min(...tpl.nodes.map((n) => n.pos![0])) : 0;
  const minY = own ? Math.min(...tpl.nodes.map((n) => n.pos![1])) : 0;
  const laid = own
    ? fresh.map((n, i) => ({ ...n, position: { x: tpl.nodes[i].pos![0] - minX, y: tpl.nodes[i].pos![1] - minY } }))
    : arrange(fresh, freshEdges);
  const left = nodes.length ? Math.min(...nodes.map((n) => n.position.x)) : 0;
  // Узлы-кнопки выше обычных — берём настоящую высоту, если карта её уже измерила.
  const top = nodes.length ? Math.max(...nodes.map((n) => n.position.y + (n.measured?.height ?? NODE_HEIGHT))) + GAP : 0;
  const placed = laid.map((n) => ({ ...n, position: { x: n.position.x + left, y: n.position.y + top }, selected: false }));
  return {
    nodes: [...nodes.map((n) => ({ ...n, selected: false })), ...placed],
    edges: [...edges, ...freshEdges],
    added: placed.map((n) => n.id),
  };
}
