// SPDX-License-Identifier: AGPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { tracePath } from './components/TraceEdge';

const points = (d: string) => d.split(/[ML]/).filter(Boolean).map((p) => p.split(',').map(Number) as [number, number]);

/** Каждый участок — горизонталь, вертикаль или ровно 45°. */
const octilinear = (pts: [number, number][]) =>
  pts.slice(1).every(([x, y], i) => {
    const dx = Math.abs(x - pts[i][0]);
    const dy = Math.abs(y - pts[i][1]);
    return dx < 0.11 || dy < 0.11 || Math.abs(dx - dy) < 0.11;
  });

describe('engineer traces', () => {
  it('goes from output to input with 45° chamfers: four bends', () => {
    const pts = points(tracePath(0, 0, 300, 120, 'a->b'));
    expect(pts[0]).toEqual([0, 0]);
    expect(pts[pts.length - 1]).toEqual([300, 120]);
    expect(pts.length).toBe(6);
    expect(octilinear(pts)).toBe(true);
  });

  it('is a straight line on one level', () => {
    expect(points(tracePath(0, 50, 300, 50, 'x'))).toEqual([
      [0, 50],
      [300, 50],
    ]);
  });

  it('loops around when the input is to the left of the output', () => {
    const pts = points(tracePath(400, 0, 100, 200, 'c->d'));
    expect(pts[0]).toEqual([400, 0]);
    expect(pts[pts.length - 1]).toEqual([100, 200]);
    expect(octilinear(pts)).toBe(true);
  });

  it('parallel traces get different vertical lanes', () => {
    const lane = (id: string) => points(tracePath(0, 0, 300, 120, id))[2][0];
    const lanes = new Set(['n1->n4', 'n2->n4', 'n3->n4', 'n5->n4', 'n6->n4'].map(lane));
    expect(lanes.size).toBeGreaterThan(1);
  });
});
