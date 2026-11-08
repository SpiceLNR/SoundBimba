// SPDX-License-Identifier: AGPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { canConnect, fromFlow, moveTo, newNode, simpleSorted, toFlow, wouldCreateCycle, type Project } from './graph';
import { DICTS, LANGUAGES } from './i18n';
import { applyTemplate, fromProject, TEMPLATES } from './templates';

describe('templates', () => {
  it('every template is a valid map: known nodes, proper directions, no cycles', () => {
    for (const tpl of TEMPLATES) {
      const keys = new Set(tpl.nodes.map((n) => n.key));
      expect(keys.size, tpl.id).toBe(tpl.nodes.length);
      const { nodes, edges } = applyTemplate([], [], tpl);
      // Каждый провод можно было бы протянуть руками в том же порядке.
      const built: typeof edges = [];
      for (const e of edges) {
        expect(canConnect(nodes, built, e.source, e.target), `${tpl.id}: ${e.source}->${e.target}`).toBe('ok');
        built.push(e);
      }
      expect(edges.some((e) => wouldCreateCycle(edges.filter((x) => x !== e), e.source, e.target))).toBe(false);
    }
  });

  it('simple-mode order: move a button, and the order goes into the template and back', () => {
    // Три кнопки; третью ставим первой.
    const ids = ['b1', 'b2', 'b3'];
    const order = moveTo(ids, 'b3', 'b1');
    expect(order).toEqual({ b3: 0, b1: 1, b2: 2 });
    expect(moveTo(ids, 'b1', 'b3')).toEqual({ b2: 0, b3: 1, b1: 2 });
    const nodes = ids.map((id, i) => {
      const n = newNode('sound-button', id, 0, i * 100);
      n.data.params = { ...n.data.params, simpleOrder: order[id] };
      return n;
    });
    // Сохранили как шаблон и добавили на пустую карту: номера узлов новые, порядок тот же.
    const tpl = fromProject('mine', fromFlow(nodes, []));
    const added = applyTemplate([], [], tpl).nodes;
    const sorted = simpleSorted(added.map((n) => ({ id: n.id, params: n.data.params })));
    expect(sorted.map((n) => added.findIndex((a) => a.id === n.id))).toEqual([2, 0, 1]);
    // Узлы без номера — после пронумерованных, в порядке карты.
    expect(simpleSorted([{ params: {} }, { params: { simpleOrder: 0 } }, { params: {} }]).map((x) => x.params.simpleOrder ?? 'none')).toEqual([0, 'none', 'none']);
  });

  it('every template has a name and description in all languages', () => {
    for (const tpl of TEMPLATES) {
      for (const lang of LANGUAGES) {
        expect(DICTS[lang], `${lang} ${tpl.id}`).toHaveProperty([`template.${tpl.id}.title`]);
        expect(DICTS[lang], `${lang} ${tpl.id}`).toHaveProperty([`template.${tpl.id}.desc`]);
      }
    }
  });

  it('adds to the map below existing nodes without touching them', () => {
    const existing = toFlow({
      schema: 1,
      nodes: [
        { id: 'n1', type: 'mic-input', pos: [100, 50], params: {} },
        { id: 'n7', type: 'speaker-out', pos: [400, 300], params: {} },
      ],
      edges: [{ from: 'n1', to: 'n7' }],
    } satisfies Project);
    const tpl = TEMPLATES.find((t) => t.id === 'game-to-mic')!;
    const r = applyTemplate(existing.nodes, existing.edges, tpl);
    expect(r.nodes).toHaveLength(2 + tpl.nodes.length);
    expect(r.added).toEqual(['n8', 'n9', 'n10', 'n11']);
    const old = r.nodes.filter((n) => !r.added.includes(n.id));
    expect(old.map((n) => n.position)).toEqual(existing.nodes.map((n) => n.position));
    for (const id of r.added) expect(r.nodes.find((n) => n.id === id)!.position.y).toBeGreaterThan(300);
    // Получившийся проект — обычный проект SoundBimba.
    const p = fromFlow(r.nodes, r.edges);
    expect(new Set(p.nodes.map((n) => n.id)).size).toBe(p.nodes.length);
    expect(p.edges).toHaveLength(1 + tpl.edges.length);
  });

  it('own templates keep their layout and wires', () => {
    const own = fromProject('mine', {
      schema: 1,
      nodes: [
        { id: 'a', type: 'mic-input', pos: [500, 300], params: { device: 'usb' } },
        { id: 'b', type: 'speaker-out', pos: [900, 420], params: {} },
      ],
      edges: [{ from: 'a', to: 'b' }],
    });
    const r = applyTemplate([], [], own);
    expect(r.nodes.map((n) => n.position)).toEqual([
      { x: 0, y: 0 },
      { x: 400, y: 120 },
    ]);
    expect(r.nodes[0].data.params.device).toBe('usb');
    expect(r.edges.map((e) => [e.source, e.target])).toEqual([['n1', 'n2']]);
  });

  it('keeps template parameters', () => {
    const r = applyTemplate([newNode('mic-input', 'n1', 0, 0)], [], TEMPLATES.find((t) => t.id === 'music-to-stream')!);
    const vol = r.nodes.find((n) => n.data.kind === 'volume')!;
    expect(vol.data.params.gain).toBe(0.4);
  });
});
