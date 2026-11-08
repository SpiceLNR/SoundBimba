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

  // Прямоугольник узла после раскладки (размеры — как их считает раскладка).
  const box = (n: ReturnType<typeof newNode>) => {
    const button = n.data.kind === 'sound-button';
    return { x: n.position.x, y: n.position.y, w: button ? 250 : 200, h: button ? 112 : 64 };
  };
  const overlap = (a: ReturnType<typeof box>, b: ReturnType<typeof box>) =>
    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
  const noOverlaps = (laid: ReturnType<typeof newNode>[]) => {
    for (let i = 0; i < laid.length; i++)
      for (let j = i + 1; j < laid.length; j++) expect(overlap(box(laid[i]), box(laid[j])), `${laid[i].id} / ${laid[j].id}`).toBe(false);
  };

  it('a straight chain lines up on one level', () => {
    const nodes = [newNode('mic-input', 'm', 0, 0), newNode('compressor', 'c', 0, 300), newNode('denoise', 'd', 0, 50), newNode('virtual-out', 'o', 0, 900)];
    const edges = [
      { id: 'm->c', source: 'm', target: 'c' },
      { id: 'c->d', source: 'c', target: 'd' },
      { id: 'd->o', source: 'd', target: 'o' },
    ];
    const laid = arrange(nodes, edges);
    const ys = new Set(laid.map((n) => n.position.y));
    expect(ys.size).toBe(1);
  });

  it('separate schemes get their own bands, nothing overlaps, tall buttons included', () => {
    const nodes = [
      newNode('sound-button', 'b1', 0, 0),
      newNode('sound-button', 'b2', 0, 10),
      newNode('sound-button', 'b3', 0, 20),
      newNode('speaker-out', 'h', 0, 0),
      newNode('mic-input', 'm', 0, 500),
      newNode('volume', 'v', 0, 500),
      newNode('virtual-out', 'o', 0, 500),
      newNode('app-source', 'lonely', 0, 0),
    ];
    const edges = [
      { id: 'b1->h', source: 'b1', target: 'h' },
      { id: 'b2->h', source: 'b2', target: 'h' },
      { id: 'b3->h', source: 'b3', target: 'h' },
      { id: 'm->v', source: 'm', target: 'v' },
      { id: 'v->o', source: 'v', target: 'o' },
    ];
    const laid = arrange(nodes, edges);
    noOverlaps(laid);
    const at = (id: string) => laid.find((n) => n.id === id)!;
    // Кнопки со своим выходом — выше схемы микрофона (так стояли на карте); схемы не перемешаны.
    const buttonsBottom = Math.max(...['b1', 'b2', 'b3', 'h'].map((id) => box(at(id)).y + box(at(id)).h));
    expect(Math.min(...['m', 'v', 'o'].map((id) => at(id).position.y))).toBeGreaterThan(buttonsBottom);
    // Кнопки — плотная стопка, выход — ровно напротив её середины.
    const centre = (id: string) => box(at(id)).y + box(at(id)).h / 2;
    expect(Math.abs(centre('h') - centre('b2'))).toBeLessThan(1);
    // Неподключённый — ниже всех.
    expect(at('lonely').position.y).toBeGreaterThan(at('o').position.y);
  });

  it('columns: buttons, then sources, then processing, then outputs', () => {
    const nodes = [
      newNode('mic-input', 'm', 0, 0),
      newNode('sound-button', 'b', 0, 400),
      newNode('volume', 'v', 0, 0),
      newNode('speaker-out', 'h', 0, 0),
      newNode('sound-button', 'lb', 0, 900),
      newNode('app-source', 'la', 0, -900),
    ];
    const edges = [
      { id: 'm->v', source: 'm', target: 'v' },
      { id: 'b->v', source: 'b', target: 'v' },
      { id: 'v->h', source: 'v', target: 'h' },
    ];
    const laid = arrange(nodes, edges);
    noOverlaps(laid);
    const x = (id: string) => laid.find((n) => n.id === id)!.position.x;
    expect(x('b')).toBeLessThan(x('m'));
    expect(x('m')).toBeLessThan(x('v'));
    expect(x('v')).toBeLessThan(x('h'));
    // Неподключённые — в столбцах своего типа.
    expect(x('lb')).toBe(x('b'));
    expect(x('la')).toBe(x('m'));
  });

  it('lays out the author’s own map like they did by hand', () => {
    // Скриншот автора 2026-11-08: 6 кнопок → громкость; chrome → громкость → наушники и микрофон;
    // громкость кнопок → наушники и микрофон; hydra → громкость → микрофон; микрофон → морфер → микрофон.
    const buttons = ['b1', 'b2', 'b3', 'b4', 'b5', 'b6'];
    const nodes = [
      ...buttons.map((id, i) => newNode('sound-button', id, 0, i * 120)),
      newNode('volume', 'bv', 300, 150),
      newNode('app-source', 'chrome', 300, 70),
      newNode('volume', 'cv', 600, 70),
      newNode('app-source', 'hydra', 300, 220),
      newNode('volume', 'hv', 600, 220),
      newNode('mic-input', 'mic', 300, 290),
      newNode('voice-morpher', 'morph', 600, 290),
      newNode('speaker-out', 'phones', 900, 70),
      newNode('virtual-out', 'cable', 900, 220),
    ];
    const pairs: [string, string][] = [
      ...buttons.map((b) => [b, 'bv'] as [string, string]),
      ['chrome', 'cv'],
      ['cv', 'phones'],
      ['cv', 'cable'],
      ['bv', 'phones'],
      ['bv', 'cable'],
      ['hydra', 'hv'],
      ['hv', 'cable'],
      ['mic', 'morph'],
      ['morph', 'cable'],
    ];
    const laid = arrange(nodes, pairs.map(([source, target]) => ({ id: `${source}->${target}`, source, target })));
    noOverlaps(laid);
    const at = (id: string) => laid.find((n) => n.id === id)!.position;
    // Столбцы: кнопки | источники и громкость кнопок | обработка | выходы.
    for (const id of ['chrome', 'hydra', 'mic']) expect(at(id).x).toBe(at('bv').x);
    expect(at('b1').x).toBeLessThan(at('bv').x);
    for (const id of ['hv', 'morph']) expect(at(id).x).toBe(at('cv').x);
    expect(at('cv').x).toBeLessThan(at('phones').x);
    expect(at('cable').x).toBe(at('phones').x);
    // Цепочки — ровными строками, выходы — на строке ближайшего соседа.
    expect(at('cv').y).toBe(at('chrome').y);
    expect(at('hv').y).toBe(at('hydra').y);
    expect(at('morph').y).toBe(at('mic').y);
    expect([at('cv').y, at('hv').y, at('morph').y]).toContain(at('phones').y);
    expect([at('cv').y, at('hv').y, at('morph').y]).toContain(at('cable').y);
    // Кнопки не растягивают схему: четыре строки справа — компактно, кнопки — плотной стопкой.
    const rightPart = ['bv', 'chrome', 'hydra', 'mic'].map((id) => at(id).y);
    expect(Math.max(...rightPart) - Math.min(...rightPart)).toBeLessThan(4 * 100);
    expect(at('b2').y - at('b1').y).toBeLessThan(130);
    // Громкость кнопок — в середине столбца и ровно напротив середины стопки кнопок; вся правая
    // часть — посередине стопки, а не у её верха.
    const order = ['bv', 'chrome', 'hydra', 'mic'].sort((p, q) => at(p).y - at(q).y);
    expect(order.indexOf('bv')).toBe(1);
    const stackMid = (at('b1').y + at('b6').y + 112) / 2;
    expect(Math.abs(at('bv').y + 32 - stackMid)).toBeLessThan(1);
    const rightMid = (Math.min(...rightPart) + Math.max(...rightPart) + 64) / 2;
    expect(Math.abs(rightMid - stackMid)).toBeLessThan(80);
  });

  it('without buttons sources stay in the first column', () => {
    const laid = arrange([newNode('mic-input', 'm', 0, 0), newNode('speaker-out', 'h', 0, 0)], [{ id: 'm->h', source: 'm', target: 'h' }]);
    expect(laid.find((n) => n.id === 'm')!.position.x).toBe(0);
  });

  it('a long wire gets a corridor instead of running through a node', () => {
    // s → v1 → v2 → o и ещё s → o напрямую: прямой провод не должен идти сквозь v1/v2.
    const nodes = [newNode('app-source', 's', 0, 0), newNode('volume', 'v1', 0, 0), newNode('volume', 'v2', 0, 0), newNode('virtual-out', 'o', 0, 0), newNode('mic-input', 'm', 0, 0), newNode('speaker-out', 'h', 0, 0)];
    const edges = [
      { id: 's->v1', source: 's', target: 'v1' },
      { id: 'v1->v2', source: 'v1', target: 'v2' },
      { id: 'v2->o', source: 'v2', target: 'o' },
      { id: 's->h', source: 's', target: 'h' },
      { id: 'm->v2', source: 'm', target: 'v2' },
    ];
    const laid = arrange(nodes, edges);
    noOverlaps(laid);
    const at = (id: string) => laid.find((n) => n.id === id)!;
    // Провод s → h проходит по высоте середины s и h; v1 не должен стоять на этой линии.
    const wireY = (box(at('s')).y + 32 + box(at('h')).y + 32) / 2;
    const v1 = box(at('v1'));
    expect(wireY < v1.y || wireY > v1.y + v1.h).toBe(true);
  });

  it('untangles a crossing-prone graph', () => {
    // Три источника и три выхода, перепутанные на карте.
    const nodes = [
      newNode('app-source', 'a', 0, 0),
      newNode('app-source', 'b', 0, 100),
      newNode('app-source', 'c', 0, 200),
      newNode('volume', 'va', 0, 0),
      newNode('volume', 'vb', 0, 100),
      newNode('volume', 'vc', 0, 200),
      newNode('speaker-out', 'x', 0, 0),
      newNode('virtual-out', 'y', 0, 100),
      newNode('speaker-out', 'z', 0, 200),
    ];
    const edges = [
      { id: 'a->vc', source: 'a', target: 'vc' },
      { id: 'b->va', source: 'b', target: 'va' },
      { id: 'c->vb', source: 'c', target: 'vb' },
      { id: 'va->z', source: 'va', target: 'z' },
      { id: 'vb->x', source: 'vb', target: 'x' },
      { id: 'vc->y', source: 'vc', target: 'y' },
      { id: 'a->x', source: 'a', target: 'x' },
    ];
    const laid = arrange(nodes, edges);
    noOverlaps(laid);
    const y = (id: string) => laid.find((n) => n.id === id)!.position.y;
    const crosses = (p: [string, string], q: [string, string]) => (y(p[0]) - y(q[0])) * (y(p[1]) - y(q[1])) < 0;
    const pairs = edges.map((e) => [e.source, e.target] as [string, string]);
    const first = pairs.filter(([, t]) => t.startsWith('v'));
    const second = pairs.filter(([s]) => s.startsWith('v'));
    let n = 0;
    for (const layer of [first, second])
      for (let i = 0; i < layer.length; i++) for (let j = i + 1; j < layer.length; j++) if (crosses(layer[i], layer[j])) n++;
    expect(n).toBe(0);
  });

  it('describes volume controls as “from → to” for simple mode', () => {
    const t = (k: string) => translate('ru', k);
    const p: Project = {
      ...project,
      nodes: [...project.nodes, { id: 'b1', type: 'sound-button', pos: [0, 0], params: { file: 'C:\\Sounds\\horn.mp3' } }],
      edges: [...project.edges, { from: 'b1', to: 'n3' }],
    };
    expect(volumeRoutes(p, t)).toEqual([
      { id: 'n3', from: 'discord.exe, horn', to: 'Виртуальный микрофон', fromNames: ['discord.exe', 'horn'], toNames: ['Виртуальный микрофон'] },
    ]);
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
