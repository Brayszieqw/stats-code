// tests/unit/skill-registry.test.ts — SkillRegistry defaults (task 5.4).
//
// withDefaults() exposes the 8 expected ids in insertion order; get hit/miss;
// every descriptor carries id, display name, input/output schema, and invoker.
//
// _Requirements: 4.1, 4.2, 4.3, 4.7_

import { describe, it, expect } from 'vitest';
import { SkillRegistry } from '@stats-code/server';

const EXPECTED_ORDER = [
  'tableone',
  'ttest',
  'model_linear',
  'model_logistic',
  'model_cox',
  'survival_km',
  'power',
  'inspect',
];

describe('SkillRegistry.withDefaults (Requirements 4.1, 4.2, 4.3, 4.7)', () => {
  it('exposes the 8 expected skills in insertion order', () => {
    const reg = SkillRegistry.withDefaults();
    expect(reg.size).toBe(8);
    expect(reg.list().map((d) => d.skillId)).toEqual(EXPECTED_ORDER);
  });

  it('get returns the matching descriptor or undefined', () => {
    const reg = SkillRegistry.withDefaults();
    expect(reg.get('model_linear')?.skillId).toBe('model_linear');
    expect(reg.get('nonexistent')).toBeUndefined();
  });

  it('every descriptor carries id, display name, input/output schema, and invoker', () => {
    const reg = SkillRegistry.withDefaults();
    for (const d of reg.list()) {
      expect(d.skillId.length).toBeGreaterThan(0);
      expect(d.displayName.length).toBeGreaterThan(0);
      expect(d.inputSchema.type).toBe('object');
      expect(d.outputSchema.type).toBe('object');
      expect(d.invoker).toBeDefined();
      expect(['algorithm', 'native']).toContain(d.invoker.kind);
    }
  });

  it('output-level skills use algorithm invokers; power/inspect are native', () => {
    const reg = SkillRegistry.withDefaults();
    expect(reg.get('tableone')?.invoker.kind).toBe('algorithm');
    expect(reg.get('ttest')?.invoker.kind).toBe('algorithm');
    expect(reg.get('model_linear')?.invoker.kind).toBe('algorithm');
    expect(reg.get('survival_km')?.invoker.kind).toBe('algorithm');
    expect(reg.get('power')?.invoker.kind).toBe('native');
    expect(reg.get('inspect')?.invoker.kind).toBe('native');
  });

  it('register preserves first-seen insertion order and overwrites in place', () => {
    const reg = new SkillRegistry();
    for (const id of ['c', 'a', 'b']) {
      reg.register({
        skillId: id,
        displayName: id,
        inputSchema: { type: 'object' },
        outputSchema: { type: 'object' },
        invoker: { kind: 'native', run: () => Promise.reject(new Error('x')) },
      });
    }
    expect(reg.list().map((d) => d.skillId)).toEqual(['c', 'a', 'b']);
    reg.register({
      skillId: 'a',
      displayName: 'A2',
      inputSchema: { type: 'object' },
      outputSchema: { type: 'object' },
      invoker: { kind: 'native', run: () => Promise.reject(new Error('x')) },
    });
    expect(reg.size).toBe(3);
    expect(reg.get('a')?.displayName).toBe('A2');
    expect(reg.list().map((d) => d.skillId)).toEqual(['c', 'a', 'b']);
  });
});
