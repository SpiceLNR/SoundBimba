// SPDX-License-Identifier: AGPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { canConnect, fromFlow, newNode, toFlow, wouldCreateCycle, type Project } from './graph';
import { DICTS, LANGUAGES } from './i18n';
import { applyTemplate, TEMPLATES } from './templates';

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

  it('keeps template parameters', () => {
    const r = applyTemplate([newNode('mic-input', 'n1', 0, 0)], [], TEMPLATES.find((t) => t.id === 'music-to-stream')!);
    const vol = r.nodes.find((n) => n.data.kind === 'volume')!;
    expect(vol.data.params.gain).toBe(0.4);
  });
});
