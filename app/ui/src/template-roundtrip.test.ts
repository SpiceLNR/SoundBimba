// SPDX-License-Identifier: AGPL-3.0-or-later
// Шаблон хранит всё, что человек настроил в узлах: схема → шаблон → снова на карту = то же самое
// (с новыми номерами узлов и сдвигом раскладки в начало координат).
import { describe, expect, it } from 'vitest';
import { fromFlow, KINDS, type NodeKind, type Project } from './graph';
import { applyTemplate, fromProject } from './templates';

/** Каждый тип узла с настройками, отличными от умолчаний. */
const SETTINGS: Partial<Record<NodeKind, Record<string, unknown>>> = {
  'sound-button': { file: 'D:/Sounds/a.mp3', label: 'Фанфары', simpleOrder: 2 },
  'app-source': { exe: 'chrome.exe', original: 'cable', originalDevice: 'dev-1' },
  'mic-input': { device: 'mic-2', name: 'Мой микрофон', mute: true },
  volume: { gain: 0.35, pan: -0.4, name: 'Тише', simpleOrder: 1 },
  compressor: { threshold: -30, ratio: 5, attack: 2, release: 300, makeup: 6 },
  'voice-morpher': {
    pitch: 3,
    robot: 0,
    radio: 0,
    echo: 0.25,
    wobble: 0.7,
    simpleOrder: 0,
    // Свои голоса и убранные готовые — тоже в шаблоне.
    voices: [{ id: 'c1', name: 'Мой бас', params: { pitch: -4, robot: 0.3, radio: 0, echo: 0, wobble: 0 } }],
    hiddenVoices: ['cave'],
  },
  denoise: { amount: 0.4 },
  'speaker-out': { device: 'phones-1' },
  'virtual-out': { device: 'virtual-2' },
};

function project(): Project {
  const kinds = Object.keys(KINDS).filter((k) => !KINDS[k as NodeKind].insideChip) as NodeKind[];
  const nodes = kinds.map((kind, i) => ({
    id: `x${i + 1}`,
    type: kind,
    pos: [40 + i * 30, 100 + i * 20] as [number, number],
    params: { ...KINDS[kind].defaults, ...(SETTINGS[kind] ?? {}) },
  }));
  const id = (k: NodeKind) => nodes.find((n) => n.type === k)!.id;
  // Микросхема с громкостью внутри: её настройки — тоже часть шаблона.
  const chip = nodes.find((n) => n.type === 'chip')!;
  chip.params = {
    ...chip.params,
    name: 'Обработка',
    inner: {
      nodes: [
        { id: 'n1', type: 'chip-in', pos: [0, 0], params: { name: 'Голос' } },
        { id: 'n3', type: 'volume', pos: [200, 0], params: { gain: 1.5, pan: 0.5 } },
        { id: 'n2', type: 'chip-out', pos: [400, 0], params: { name: 'В стрим' } },
      ],
      edges: [
        { from: 'n1', to: 'n3' },
        { from: 'n3', to: 'n2' },
      ],
    },
  };
  const edges = [
    { from: id('mic-input'), to: id('denoise') },
    { from: id('denoise'), to: id('voice-morpher') },
    { from: id('voice-morpher'), to: chip.id, toPort: 'n1' },
    { from: chip.id, to: id('virtual-out'), fromPort: 'n2' },
    { from: id('sound-button'), to: id('volume') },
    { from: id('volume'), to: id('speaker-out') },
    { from: id('app-source'), to: id('compressor') },
    { from: id('compressor'), to: id('speaker-out') },
  ];
  return { schema: 2, nodes, edges };
}

describe('template keeps every node setting', () => {
  it('scheme → template → map gives the same nodes, settings, layout and wires', () => {
    const p = project();
    const { nodes, edges } = applyTemplate([], [], fromProject('t', p));
    const back = fromFlow(nodes, edges);
    const minX = Math.min(...p.nodes.map((n) => n.pos[0]));
    const minY = Math.min(...p.nodes.map((n) => n.pos[1]));
    expect(back.nodes.length).toBe(p.nodes.length);
    const map = new Map<string, string>();
    p.nodes.forEach((n, i) => {
      const b = back.nodes[i];
      map.set(n.id, b.id);
      expect(b.type, n.id).toBe(n.type);
      expect(b.params, n.type).toEqual(n.params);
      expect(b.pos, n.type).toEqual([n.pos[0] - minX, n.pos[1] - minY]);
    });
    const wire = (e: Project['edges'][number], m: (id: string) => string) =>
      `${m(e.from)}>${m(e.to)}:${e.fromPort ?? ''}:${e.toPort ?? ''}`;
    expect(back.edges.map((e) => wire(e, (x) => x)).sort()).toEqual(p.edges.map((e) => wire(e, (x) => map.get(x)!)).sort());
    expect(back.schema).toBe(2);
  });
});
