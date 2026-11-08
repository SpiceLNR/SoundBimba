// SPDX-License-Identifier: AGPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { arrange, canConnect, chipPins, composeRoot, flattenProject, fromFlow, newNode, patchNested, toFlow, volumeRoutes, type Project } from './graph';
import { translate } from './i18n';
import { applyTemplate, fromProject } from './templates';

/** Микрофон → микросхема «Голос» (вход → громкость → выход) → виртуальный кабель. */
const withChip: Project = {
  schema: 2,
  nodes: [
    { id: 'n1', type: 'mic-input', pos: [0, 0], params: { device: 'default' } },
    {
      id: 'n2',
      type: 'chip',
      pos: [300, 0],
      params: {
        name: 'Голос',
        inner: {
          nodes: [
            { id: 'n1', type: 'chip-in', pos: [0, 0], params: { name: 'Голос' } },
            { id: 'n2', type: 'volume', pos: [260, 0], params: { gain: 0.5, mute: false } },
            { id: 'n3', type: 'chip-out', pos: [520, 0], params: { name: '' } },
          ],
          edges: [
            { from: 'n1', to: 'n2' },
            { from: 'n2', to: 'n3' },
          ],
        },
      },
    },
    { id: 'n3', type: 'virtual-out', pos: [600, 0], params: { device: 'virtual-1' } },
  ],
  edges: [
    { from: 'n1', to: 'n2', toPort: 'n1' },
    { from: 'n2', to: 'n3', fromPort: 'n3' },
  ],
};

describe('chips', () => {
  it('old projects stay exactly as they were (schema 1, no pins)', () => {
    const old: Project = {
      schema: 1,
      nodes: [
        { id: 'n1', type: 'mic-input', pos: [0, 0], params: { device: 'default' } },
        { id: 'n2', type: 'virtual-out', pos: [300, 0], params: { device: 'virtual-1' } },
      ],
      edges: [{ from: 'n1', to: 'n2' }],
    };
    const flow = toFlow(old);
    expect(fromFlow(flow.nodes, flow.edges)).toEqual(old);
  });

  it('a project with a chip keeps its pins and gets schema 2', () => {
    const flow = toFlow(withChip);
    expect(flow.edges[0]).toMatchObject({ source: 'n1', target: 'n2', targetHandle: 'n1' });
    expect(fromFlow(flow.nodes, flow.edges)).toEqual(withChip);
  });

  it('pins come from the chip-in / chip-out nodes inside, top to bottom', () => {
    const chip = newNode('chip', 'c', 0, 0);
    expect(chipPins(chip.data.params)).toEqual({ ins: [{ id: 'n1', name: '' }], outs: [{ id: 'n2', name: '' }] });
    expect(chipPins(withChip.nodes[1].params)).toEqual({ ins: [{ id: 'n1', name: 'Голос' }], outs: [{ id: 'n3', name: '' }] });
  });

  it('wires go to a chip only through a pin', () => {
    const { nodes } = toFlow(withChip);
    expect(canConnect(nodes, [], 'n1', 'n2')).toBe('invalid');
    expect(canConnect(nodes, [], 'n1', 'n2', null, 'n1')).toBe('ok');
    expect(canConnect(nodes, [], 'n2', 'n3', 'n3', null)).toBe('ok');
    expect(canConnect(nodes, [], 'n1', 'n3', null, 'n1')).toBe('invalid');
  });

  it('editing inside a chip saves the whole project; wires to removed pins are dropped', () => {
    const top = toFlow(withChip);
    const inside = toFlow({ schema: 1, ...(withChip.nodes[1].params.inner as Omit<Project, 'schema'>) });
    // Внутри: громкость стала 0.8, выход микросхемы удалили.
    const nodes = inside.nodes.filter((n) => n.id !== 'n3').map((n) => (n.id === 'n2' ? { ...n, data: { ...n.data, params: { gain: 0.8, mute: false } } } : n));
    const edges = inside.edges.filter((e) => e.target !== 'n3');
    const root = composeRoot([{ chipId: 'n2', nodes: top.nodes, edges: top.edges }], nodes, edges);
    const inner = root.nodes[1].params.inner as Project;
    expect(inner.nodes.map((n) => n.id)).toEqual(['n1', 'n2']);
    expect(inner.nodes[1].params.gain).toBe(0.8);
    // Провод от удалённой ножки наружу пропал, ко входу — остался.
    expect(root.edges).toEqual([{ from: 'n1', to: 'n2', toPort: 'n1' }]);
    expect(root.schema).toBe(2);
  });

  it('a chip saved as a template keeps its pins when added', () => {
    const flow = toFlow(withChip);
    const tpl = fromProject('mine', fromFlow(flow.nodes, flow.edges));
    const r = applyTemplate([], [], tpl);
    const p = fromFlow(r.nodes, r.edges);
    expect(p.edges.map((e) => [e.fromPort, e.toPort])).toEqual([
      [undefined, 'n1'],
      ['n3', undefined],
    ]);
  });

  it('simple mode sees what is inside chips, by the same ids as the sound engine', () => {
    // Кнопка внутри микросхемы, её громкость — тоже внутри.
    const inner = withChip.nodes[1].params.inner as Project;
    const p: Project = {
      ...withChip,
      nodes: [
        withChip.nodes[0],
        {
          ...withChip.nodes[1],
          params: {
            ...withChip.nodes[1].params,
            inner: { nodes: [...inner.nodes, { id: 'n4', type: 'sound-button', pos: [0, 200], params: { label: 'Горн' } }], edges: [...inner.edges, { from: 'n4', to: 'n2' }] },
          },
        },
        withChip.nodes[2],
      ],
    };
    const flat = flattenProject(p);
    // Те же id, что даёт Rust (graph::flatten): «микросхема/узел».
    expect(flat.nodes.map((n) => n.id)).toEqual(['n1', 'n2/n1', 'n2/n2', 'n2/n3', 'n2/n4', 'n3']);
    expect(flat.edges.map((e) => `${e.from}>${e.to}`)).toEqual(['n2/n1>n2/n2', 'n2/n2>n2/n3', 'n2/n4>n2/n2', 'n1>n2/n1', 'n2/n3>n3']);
    // «Откуда → куда» у громкости внутри — сквозь ножки: микрофон и кнопка → виртуальный микрофон.
    const t = (k: string) => translate('ru', k);
    expect(volumeRoutes(flat, t)).toEqual([
      { id: 'n2/n2', from: 'Микрофон, Горн', to: 'Виртуальный микрофон', fromNames: ['Микрофон', 'Горн'], toNames: ['Виртуальный микрофон'] },
    ]);
  });

  it('a change from simple mode lands inside the chip', () => {
    const params = patchNested(withChip.nodes[1].params, ['n2'], { gain: 1.5 });
    const inner = params.inner as Project;
    expect(inner.nodes[1].params).toEqual({ gain: 1.5, mute: false });
    expect(inner.nodes[0]).toEqual((withChip.nodes[1].params.inner as Project).nodes[0]);
  });

  it('arrange: a chip without inputs stands with the sources, rows stay straight', () => {
    // Как на карте автора (2026-11-08): микросхема с кнопками внутри (только выход) → громкость → наушники.
    const chip = newNode('chip', 'c', 0, 0);
    chip.data.params = { ...chip.data.params, inner: { nodes: [{ id: 'o', type: 'chip-out', pos: [0, 0], params: {} }], edges: [] } };
    const nodes = [newNode('app-source', 'a', 0, 0), newNode('volume', 'av', 0, 0), chip, newNode('volume', 'cv', 0, 0), newNode('speaker-out', 'h', 0, 0)];
    const edges = [
      { id: 'a->av', source: 'a', target: 'av' },
      { id: 'av->h', source: 'av', target: 'h' },
      { id: 'c->cv', source: 'c', target: 'cv', sourceHandle: 'o' },
      { id: 'cv->h', source: 'cv', target: 'h' },
    ];
    const laid = arrange(nodes, edges);
    const at = (id: string) => laid.find((n) => n.id === id)!.position;
    expect(at('c').x).toBe(at('a').x);
    expect(at('cv').x).toBe(at('av').x);
    // Одна строка — середины на одной высоте (микросхема выше: строка на ножку).
    expect(at('cv').y + 32).toBe(at('c').y + (64 + 24) / 2);
    expect(at('h').x).toBeGreaterThan(at('cv').x);
  });
});
