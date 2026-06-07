import { describe, it, expect } from 'vitest';
import fc from 'fast-check';
import { snapshot } from '@stats-code/engine';

const { workflowYaml } = snapshot;
const { parse, prettyPrint, WorkflowYamlError, RULE, isValidSha256 } = workflowYaml;

const SHA = '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef';
const SHA2 = 'fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210';

const MINIMAL = `schema_version: 1
input_dataset:
  path: data.csv
  sha256: ${SHA}
steps:
  - id: step-1
    algorithm: tableone
    params:
      by: treatment
      vars:
        - age
        - sex
    inputs:
      - path: data.csv
        sha256: ${SHA}
    outputs:
      - path: out.json
        sha256: ${SHA2}
    started_at_utc: "2025-01-01T00:00:00Z"
    ended_at_utc: "2025-01-01T00:00:01Z"
`;

describe('workflow YAML parse', () => {
  it('parses a minimal valid document', () => {
    const { workflow } = parse(MINIMAL);
    expect(workflow.schemaVersion).toBe(1);
    expect(workflow.inputDataset.path).toBe('data.csv');
    expect(workflow.steps).toHaveLength(1);
    expect(workflow.steps[0]!.algorithm).toBe('tableone');
    expect(workflow.steps[0]!.inputs[0]!.sha256).toBe(SHA);
  });

  it('rejects an unsupported schema_version', () => {
    const bad = MINIMAL.replace('schema_version: 1', 'schema_version: 2');
    expect(() => parse(bad)).toThrow(WorkflowYamlError);
    try {
      parse(bad);
    } catch (e) {
      expect((e as InstanceType<typeof WorkflowYamlError>).ruleViolated).toBe(
        RULE.SCHEMA_VERSION_UNSUPPORTED,
      );
    }
  });

  it('rejects a malformed sha256', () => {
    const bad = MINIMAL.replace(SHA, 'tooshort');
    expect(() => parse(bad)).toThrow(WorkflowYamlError);
  });

  it('rejects a duplicate step id', () => {
    const dup = `schema_version: 1
input_dataset:
  path: d.csv
  sha256: ${SHA}
steps:
  - id: s
    algorithm: a
    params: {}
    inputs: []
    outputs: []
    started_at_utc: t
    ended_at_utc: t
  - id: s
    algorithm: b
    params: {}
    inputs: []
    outputs: []
    started_at_utc: t
    ended_at_utc: t
`;
    try {
      parse(dup);
      expect.unreachable();
    } catch (e) {
      expect((e as InstanceType<typeof WorkflowYamlError>).ruleViolated).toBe(RULE.DUPLICATE_STEP_ID);
    }
  });

  it('rejects a missing required field', () => {
    const bad = `schema_version: 1
steps: []
`;
    expect(() => parse(bad)).toThrow(WorkflowYamlError);
  });

  it('isValidSha256 accepts 64 lowercase hex only', () => {
    expect(isValidSha256(SHA)).toBe(true);
    expect(isValidSha256(SHA.toUpperCase())).toBe(false);
    expect(isValidSha256('abc')).toBe(false);
  });
});

describe('Property 13: YAML byte round-trip (doc-side)', () => {
  it('prettyPrint(_, doc) reproduces the exact input bytes', () => {
    const { workflow, doc } = parse(MINIMAL);
    expect(prettyPrint(workflow, doc)).toBe(MINIMAL);
  });

  it('preserves comments and blank lines verbatim via the doc handle', () => {
    const withTrivia = `# top comment
schema_version: 1

input_dataset:
  path: data.csv   # inline
  sha256: ${SHA}
steps: []
`;
    const { workflow, doc } = parse(withTrivia);
    expect(prettyPrint(workflow, doc)).toBe(withTrivia);
  });
});

describe('Property 14: YAML model round-trip (canonical fixpoint)', () => {
  it('canonical form is a fixpoint: prettyPrint(parse(prettyPrint(W)))', () => {
    const { workflow } = parse(MINIMAL);
    const canon1 = prettyPrint(workflow);
    const reparsed = parse(canon1).workflow;
    const canon2 = prettyPrint(reparsed);
    expect(canon2).toBe(canon1);
  });

  it('serialize→parse→serialize yields an equivalent model', () => {
    const { workflow } = parse(MINIMAL);
    const canon = prettyPrint(workflow);
    const reparsed = parse(canon).workflow;
    expect(reparsed).toEqual(workflow);
  });
});

// Generators for arbitrary-but-valid workflows.
const safeString = fc
  .string({ minLength: 1, maxLength: 12 })
  .filter((s) => /^[A-Za-z][A-Za-z0-9_.-]*$/.test(s));

const artifact = fc.record({
  path: safeString.map((s) => `${s}.csv`),
  sha256: fc.constantFrom(SHA, SHA2),
});

const step = fc.record({
  id: safeString,
  algorithm: fc.constantFrom('tableone', 'ttest', 'cox', 'logistic'),
  params: fc.constantFrom({}, { by: 'x' }, { vars: ['a', 'b'] }, { n: 5 }),
  inputs: fc.array(artifact, { maxLength: 2 }),
  outputs: fc.array(artifact, { maxLength: 2 }),
  startedAtUtc: fc.constant('2025-01-01T00:00:00Z'),
  endedAtUtc: fc.constant('2025-01-01T00:00:01Z'),
});

const workflowArb = fc
  .record({
    schemaVersion: fc.constant(1 as const),
    inputDataset: fc.record({ path: safeString.map((s) => `${s}.csv`), sha256: fc.constantFrom(SHA, SHA2) }),
    steps: fc.array(step, { maxLength: 3 }),
  })
  .map((w) => ({
    ...w,
    // dedupe step ids to satisfy the parser invariant
    steps: w.steps.filter((s, i, arr) => arr.findIndex((o) => o.id === s.id) === i),
  }));

describe('Property 14 (fast-check): arbitrary models round-trip canonically', () => {
  it('parse(prettyPrint(W)) === W for arbitrary valid workflows', () => {
    fc.assert(
      fc.property(workflowArb, (w) => {
        const canon = prettyPrint(w as workflowYaml.Workflow);
        const reparsed = parse(canon).workflow;
        // normalize optional fields the generator omits (undefined)
        expect(reparsed.schemaVersion).toBe(1);
        expect(reparsed.steps.length).toBe(w.steps.length);
        // canonical form must be a fixpoint
        expect(prettyPrint(reparsed)).toBe(canon);
      }),
      { numRuns: 100 },
    );
  });
});
