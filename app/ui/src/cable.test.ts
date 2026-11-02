// SPDX-License-Identifier: AGPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import type { AudioDevice } from './api';
import { foundCable } from './cable';

const dev = (name: string, render: boolean, cable: string | null): AudioDevice => ({ id: name, name, render, isDefault: false, cable });

describe('cable wizard', () => {
  it('finds the cable only when both of its sides are present', () => {
    expect(foundCable([dev('CABLE Input', true, 'VB-CABLE'), dev('CABLE Output', false, 'VB-CABLE')])).toBe('VB-CABLE');
    expect(foundCable([dev('CABLE Input', true, 'VB-CABLE')])).toBeNull();
    expect(foundCable([dev('CABLE Output', false, 'VB-CABLE')])).toBeNull();
    expect(foundCable([dev('Speakers', true, null), dev('Microphone', false, null)])).toBeNull();
    expect(foundCable([])).toBeNull();
  });
});
