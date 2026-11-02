// SPDX-License-Identifier: AGPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { arrange, canConnect, fromFlow, newNode, nextNodeId, summary, toFlow, volumeRoutes, wouldCreateCycle, type Project } from './graph';
import { formatTime, ringFraction } from './components/ButtonDisc';
import { translate } from './i18n';

// Тот же проект, что в crates/graph (default_project) и в примере из ARCHITECTURE.md.
const project: Project = {
  schema: 1,
  nodes: [
    { id: 'n1', type: 'app-source', pos: [0, 0], params: { exe: 'discord.exe' } },
    { id: 'n2', type: 'mic-input', pos: [0, 160], params: { device: 'default' } },
    { id: 'n3', type: 'volume', pos: [260, 0], params: { gain: 1, mute: false } },
    { id: 'n4', type: 'virtual-out', pos: [520, 60], params: { device: 'virtual-1' } },
  ],
  edges: [
    { from: 'n1', to: 'n3' },
    { from: 'n2', to: 'n4' },
    { from: 'n3', to: 'n4' },
  ],
};

describe('graph', () => {
  it('converts to the map and back without changes', () => {
    const flow = toFlow(project);
    expect(fromFlow(flow.nodes, flow.edges)).toEqual(project);
  });

  it('keeps control port kind', () => {
    const p: Project = { ...project, edges: [{ from: 'n1', to: 'n3', kind: 'control' }] };
    const flow = toFlow(p);
    expect(fromFlow(flow.nodes, flow.edges).edges[0]).toEqual({ from: 'n1', to: 'n3', kind: 'control' });
  });

  it('checks connections: direction, duplicates, cycles', () => {
    const { nodes, edges } = toFlow(project);
    expect(canConnect(nodes, edges, 'n2', 'n3')).toBe('ok');
    expect(canConnect(nodes, edges, 'n4', 'n3')).toBe('invalid'); // у выхода нет выхода
    expect(canConnect(nodes, edges, 'n3', 'n1')).toBe('invalid'); // у источника нет входа
    expect(canConnect(nodes, edges, 'n1', 'n3')).toBe('invalid'); // уже соединены
    const v2 = newNode('volume', 'n5', 0, 0);
    const withV2 = [...nodes, v2];
    const e2 = [...edges, { id: 'n3->n5', source: 'n3', target: 'n5' }];
    expect(canConnect(withV2, e2, 'n5', 'n3')).toBe('cycle');
  });

  it('detects cycles through several nodes', () => {
    const edges = [
      { source: 'a', target: 'b' },
      { source: 'b', target: 'c' },
    ];
    expect(wouldCreateCycle(edges, 'c', 'a')).toBe(true);
    expect(wouldCreateCycle(edges, 'a', 'c')).toBe(false);
    expect(wouldCreateCycle(edges, 'a', 'a')).toBe(true);
  });

  it('gives new unique ids', () => {
    expect(nextNodeId([{ id: 'n1' }, { id: 'n7' }, { id: 'x' }])).toBe('n8');
    expect(nextNodeId([])).toBe('n1');
  });

  it('arranges left to right: sources, processing, outputs', () => {
    const { nodes, edges } = toFlow(project);
    const laid = arrange(nodes, edges);
    const x = (id: string) => laid.find((n) => n.id === id)!.position.x;
    expect(x('n1')).toBeLessThan(x('n3'));
    expect(x('n3')).toBeLessThan(x('n4'));
    expect(x('n2')).toBeLessThan(x('n4'));
  });

  it('puts unconnected nodes into columns by type, below the connected part', () => {
    const { nodes, edges } = toFlow(project);
    const extra = [newNode('mic-input', 'm2', 900, 900), newNode('volume', 'v2', -50, 400), newNode('speaker-out', 's2', 0, 0)];
    const laid = arrange([...nodes, ...extra], edges);
    const at = (id: string) => laid.find((n) => n.id === id)!.position;
    // Не в один ряд: источник левее обработки, обработка левее выхода.
    expect(at('m2').x).toBeLessThan(at('v2').x);
    expect(at('v2').x).toBeLessThan(at('s2').x);
    // Те же столбцы, что у соединённых узлов того же типа.
    expect(at('m2').x).toBe(at('n1').x);
    expect(at('v2').x).toBe(at('n3').x);
    expect(at('s2').x).toBe(at('n4').x);
    // И ниже соединённой схемы.
    const bottom = Math.max(...['n1', 'n2', 'n3', 'n4'].map((id) => at(id).y));
    for (const id of ['m2', 'v2', 's2']) expect(at(id).y).toBeGreaterThan(bottom);
  });

  it('only unconnected nodes still form type columns', () => {
    const nodes = [newNode('speaker-out', 'a', 0, 0), newNode('app-source', 'b', 0, 0), newNode('volume', 'c', 0, 0), newNode('mic-input', 'd', 0, 0)];
    const laid = arrange(nodes, []);
    const at = (id: string) => laid.find((n) => n.id === id)!.position;
    expect(at('b').x).toBe(at('d').x);
    expect(at('b').x).toBeLessThan(at('c').x);
    expect(at('c').x).toBeLessThan(at('a').x);
    expect(at('b').y).not.toBe(at('d').y);
  });

  it('a chain of processing nodes steps to the right, outputs stay last', () => {
    const nodes = [
      newNode('app-source', 's', 0, 0),
      newNode('volume', 'v1', 0, 0),
      newNode('volume', 'v2', 0, 0),
      newNode('virtual-out', 'o', 0, 0),
      newNode('speaker-out', 'h', 0, 0),
    ];
    const edges = [
      { id: 's->v1', source: 's', target: 'v1' },
      { id: 'v1->v2', source: 'v1', target: 'v2' },
      { id: 'v2->o', source: 'v2', target: 'o' },
      { id: 's->h', source: 's', target: 'h' },
    ];
    const laid = arrange(nodes, edges);
    const x = (id: string) => laid.find((n) => n.id === id)!.position.x;
    expect(x('s')).toBeLessThan(x('v1'));
    expect(x('v1')).toBeLessThan(x('v2'));
    expect(x('v2')).toBeLessThan(x('o'));
    expect(x('h')).toBe(x('o'));
  });

  it('orders a column so wires do not cross', () => {
    // a → B, b → A: выходы встают в том же порядке, что их источники.
    const nodes = [newNode('app-source', 'a', 0, 0), newNode('mic-input', 'b', 0, 100), newNode('speaker-out', 'A', 0, 0), newNode('virtual-out', 'B', 0, 100)];
    const edges = [
      { id: 'a->B', source: 'a', target: 'B' },
      { id: 'b->A', source: 'b', target: 'A' },
    ];
    const laid = arrange(nodes, edges);
    const y = (id: string) => laid.find((n) => n.id === id)!.position.y;
    expect(y('a') < y('b')).toBe(y('B') < y('A'));
  });

  it('describes volume controls as “from → to” for simple mode', () => {
    const t = (k: string) => translate('ru', k);
    const p: Project = {
      ...project,
      nodes: [...project.nodes, { id: 'b1', type: 'sound-button', pos: [0, 0], params: { file: 'C:\\Sounds\\horn.mp3' } }],
      edges: [...project.edges, { from: 'b1', to: 'n3' }],
    };
    expect(volumeRoutes(p, t)).toEqual([{ id: 'n3', from: 'discord.exe, horn', to: 'Виртуальный микрофон' }]);
  });

  it('writes short node subtitles', () => {
    const t = (k: string) => translate('ru', k);
    expect(summary('volume', { gain: 1.5, mute: true }, t)).toBe('150 % · звук выключен');
    expect(summary('app-source', { exe: '' }, t)).toBe('программа не выбрана');
    expect(summary('mic-input', { device: 'default' }, t)).toBe('по умолчанию');
  });

  it('formats button time and maps the ring to the track', () => {
    expect(formatTime(0)).toBe('0:00');
    expect(formatTime(75.9)).toBe('1:15');
    expect(formatTime(3725)).toBe('1:02:05');
    expect(ringFraction(0, -10)).toBeCloseTo(0); // верх — начало
    expect(ringFraction(10, 0)).toBeCloseTo(0.25); // справа — четверть
    expect(ringFraction(0, 10)).toBeCloseTo(0.5); // низ — середина
    expect(ringFraction(-10, 0)).toBeCloseTo(0.75); // слева — три четверти
  });
});
