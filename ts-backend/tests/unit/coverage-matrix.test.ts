import { describe, it, expect } from 'vitest';
import { coverage as cov } from '@stats-code/engine';

const {
  getLoadedMatrix,
  parseCoverageMatrix,
  lookup,
  coverageState,
  checkConsistency,
  cellKey,
  REQUIRED_SOFTWARE,
} = cov;

describe('coverage matrix loader', () => {
  it('parses the embedded matrix with all 17 algorithms and complete cells', () => {
    const m = getLoadedMatrix();
    expect(m.schema_version).toBe(1);
    expect(m.algorithms).toHaveLength(17);
    for (const entry of m.algorithms) {
      for (const sw of REQUIRED_SOFTWARE) {
        expect(entry.coverage[sw]).toBeDefined();
        expect(entry.reference[sw]?.version).toBeTruthy();
      }
    }
  });

  it('injects the engine version in place of the placeholder', () => {
    const m = getLoadedMatrix();
    expect(m.release_version).not.toBe('0.0.0-build-injected');
  });

  it('lookup is case-sensitive exact match', () => {
    const m = getLoadedMatrix();
    expect(lookup(m, 'tableone')?.id).toBe('tableone');
    expect(lookup(m, 'TableOne')).toBeUndefined();
    expect(lookup(m, 'does_not_exist')).toBeUndefined();
  });

  it('returns expected coverage states', () => {
    const m = getLoadedMatrix();
    expect(coverageState(m, 'tableone', 'R')).toBe('live');
    expect(coverageState(m, 'tableone', 'SAS')).toBe('recorded');
    expect(coverageState(m, 'standardization', 'R')).toBe('sidecar_only');
    expect(coverageState(m, 'standardization', 'SPSS')).toBe('none');
    expect(coverageState(m, 'does_not_exist', 'R')).toBeUndefined();
  });

  it('preserves declared order (tableone, ttest, anova first)', () => {
    const m = getLoadedMatrix();
    expect(m.algorithms.slice(0, 3).map((e) => e.id)).toEqual(['tableone', 'ttest', 'anova']);
  });

  it('marks cox and logistic as iterative', () => {
    const m = getLoadedMatrix();
    expect(lookup(m, 'cox')?.iterative).toBe(true);
    expect(lookup(m, 'logistic')?.iterative).toBe(true);
    expect(lookup(m, 'tableone')?.iterative).toBe(false);
  });

  it('splits ui_runnable honestly: 10 runnable vs 7 engine-level only (G2)', () => {
    const m = getLoadedMatrix();
    const runnable = m.algorithms.filter((e) => e.ui_runnable).map((e) => e.id).sort();
    const engineOnly = m.algorithms.filter((e) => !e.ui_runnable).map((e) => e.id).sort();
    // The 8 dispatched skills + the two power designs reachable through the
    // merged `power` skill (test_type → powerPhase3 / powerSingleArm).
    expect(runnable).toEqual([
      'anova', 'correlation', 'cox', 'kaplan_meier', 'linear', 'logistic',
      'power_phase3', 'power_single_arm', 'tableone', 'ttest',
    ]);
    // Engine-verified (parity/oracle) but no /run dispatch or configurator
    // entry — including power_phase2, which the merged skill never maps to.
    expect(engineOnly).toEqual([
      'attributable_risk', 'diagnostic_roc', 'life_table', 'nonparametric',
      'or_rr', 'power_phase2', 'standardization',
    ]);
  });

  it('defaults ui_runnable to false when a fixture omits it', () => {
    const toml = `schema_version = 1
release_version = "x"
[[algorithm]]
id = "fixture"
display_name = "Fixture"
iterative = false
[algorithm.coverage]
R = "none"
SAS = "none"
Python = "none"
SPSS = "none"
[algorithm.reference]
R = { fn = "f", version = "1" }
SAS = { proc = "P", version = "1" }
Python = { fn = "f", version = "1" }
SPSS = { proc = "P", version = "1" }
`;
    const m = parseCoverageMatrix(toml, '1.0.0');
    expect(m.algorithms[0]?.ui_runnable).toBe(false);
  });
});

describe('coverage matrix parse errors', () => {
  it('rejects malformed TOML', () => {
    expect(() => parseCoverageMatrix('schema_version = 1\n[[algorithm', '1.0.0')).toThrow();
  });

  it('rejects a wrong schema_version', () => {
    expect(() => parseCoverageMatrix('schema_version = 2\n', '1.0.0')).toThrow(/schema_version/);
  });

  it('rejects a duplicate algorithm id', () => {
    const toml = `schema_version = 1
release_version = "x"
[[algorithm]]
id = "dup"
display_name = "A"
iterative = false
[algorithm.coverage]
R = "live"
SAS = "live"
Python = "live"
SPSS = "live"
[algorithm.reference]
R = { fn = "f", version = "1" }
SAS = { proc = "p", version = "1" }
Python = { fn = "f", version = "1" }
SPSS = { proc = "p", version = "1" }
[[algorithm]]
id = "dup"
display_name = "B"
iterative = false
[algorithm.coverage]
R = "live"
SAS = "live"
Python = "live"
SPSS = "live"
[algorithm.reference]
R = { fn = "f", version = "1" }
SAS = { proc = "p", version = "1" }
Python = { fn = "f", version = "1" }
SPSS = { proc = "p", version = "1" }
`;
    expect(() => parseCoverageMatrix(toml, '1.0.0')).toThrow(/duplicate/);
  });

  it('rejects an incomplete coverage row (missing SPSS)', () => {
    const toml = `schema_version = 1
[[algorithm]]
id = "x"
display_name = "X"
iterative = false
[algorithm.coverage]
R = "live"
SAS = "live"
Python = "live"
[algorithm.reference]
R = { fn = "f", version = "1" }
SAS = { proc = "p", version = "1" }
Python = { fn = "f", version = "1" }
SPSS = { proc = "p", version = "1" }
`;
    expect(() => parseCoverageMatrix(toml, '1.0.0')).toThrow();
  });
});

describe('coverage consistency check', () => {
  it('flags a live cell with no backing live case (Req 5.2)', () => {
    const m = getLoadedMatrix();
    const errors = checkConsistency(m, {
      liveCases: new Set(),
      recordedTables: new Set(),
      templates: new Set(),
    });
    // Every live cell becomes a missing_live_case error.
    expect(errors.some((e) => e.kind === 'missing_live_case')).toBe(true);
  });

  it('reports no errors when the surface backs every cell exactly', () => {
    const m = getLoadedMatrix();
    const liveCases = new Set<string>();
    const recordedTables = new Set<string>();
    const templates = new Set<string>();
    for (const entry of m.algorithms) {
      for (const sw of REQUIRED_SOFTWARE) {
        const key = cellKey(entry.id, sw);
        switch (entry.coverage[sw]) {
          case 'live':
            liveCases.add(key);
            break;
          case 'recorded':
            recordedTables.add(key);
            break;
          case 'sidecar_only':
            templates.add(key);
            break;
          case 'none':
            break;
        }
      }
    }
    expect(checkConsistency(m, { liveCases, recordedTables, templates })).toEqual([]);
  });
});
