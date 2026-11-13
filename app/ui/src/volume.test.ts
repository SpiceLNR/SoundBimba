// SPDX-License-Identifier: AGPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { toFlow, type Project } from './graph';
import { translate } from './i18n';
import { applyTemplate, fromProject } from './templates';
import { balanceLabel, DB_MIN, dbLabel, dbToGain, gainToDb, migrateParams } from './volume';

const t = (k: string, v?: Record<string, string | number>) => translate('ru', k, v);

describe('volume', () => {
  it('slider in decibels: 0 dB is unchanged, far left is off, top is ×4', () => {
    expect(gainToDb(1)).toBe(0);
    expect(dbToGain(0)).toBe(1);
    expect(gainToDb(0)).toBe(DB_MIN);
    expect(dbToGain(DB_MIN)).toBe(0);
    expect(gainToDb(4)).toBe(12);
    expect(dbToGain(-6)).toBeCloseTo(0.5, 2);
    // Старые значения (проценты) попадают на ближайшие полдецибела.
    expect(gainToDb(0.5)).toBe(-6);
    expect(gainToDb(0.0001)).toBe(DB_MIN + 0.5);
    expect(dbLabel(1, t)).toBe('0 дБ');
    expect(dbLabel(0.5, t)).toBe('−6 дБ');
    expect(dbLabel(0, t)).toBe('выкл.');
    expect(translate('en', 'num.decimal')).toBe('.');
  });

  it('balance labels', () => {
    expect(balanceLabel(0, t)).toBe('по центру');
    expect(balanceLabel(-0.25, t)).toBe('левее 25 %');
    expect(balanceLabel(1, t)).toBe('правее 100 %');
  });

  it('old muted volume opens with the slider at off, also inside chips and templates', () => {
    expect(migrateParams('volume', { gain: 0.8, mute: true })).toEqual({ gain: 0 });
    expect(migrateParams('volume', { gain: 0.8, mute: false })).toEqual({ gain: 0.8 });
    expect(migrateParams('mic-input', { mute: true })).toEqual({ mute: true });
    const old: Project = {
      schema: 2,
      nodes: [
        { id: 'v', type: 'volume', pos: [0, 0], params: { gain: 1.2, mute: true } },
        {
          id: 'c',
          type: 'chip',
          pos: [0, 0],
          params: { inner: { nodes: [{ id: 'w', type: 'volume', pos: [0, 0], params: { gain: 0.5, mute: true } }], edges: [] } },
        },
      ],
      edges: [],
    };
    const flow = toFlow(old);
    expect(flow.nodes[0].data.params).toEqual({ gain: 0 });
    expect((flow.nodes[1].data.params.inner as Project).nodes[0].params).toEqual({ gain: 0 });
    const added = applyTemplate([], [], fromProject('x', old)).nodes;
    expect(added[0].data.params).toEqual({ gain: 0, pan: 0 });
  });
});
