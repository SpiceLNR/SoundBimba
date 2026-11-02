// SPDX-License-Identifier: AGPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { arrange, canConnect, fromFlow, newNode, nextNodeId, summary, toFlow, wouldCreateCycle, type Project } from './graph';
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

  it('writes short node subtitles', () => {
    const t = (k: string) => translate('ru', k);
    expect(summary('volume', { gain: 1.5, mute: true }, t)).toBe('150 % · звук выключен');
    expect(summary('app-source', { exe: '' }, t)).toBe('программа не выбрана');
    expect(summary('mic-input', { device: 'default' }, t)).toBe('по умолчанию');
  });
});
