import { createHash } from 'node:crypto';
import { describe, expect, it } from 'vitest';
import { auditDataset } from '@stats-code/server';

function audit(csv: string, options: {
  skillId?: string;
  args?: Record<string, unknown>;
  roles?: Record<string, unknown>;
} = {}) {
  const bytes = new TextEncoder().encode(csv);
  return auditDataset({
    datasetId: '11111111-1111-4111-8111-111111111111',
    datasetSha256: createHash('sha256').update(bytes).digest('hex'),
    bytes,
    fileName: 'data.csv',
    protocolVersion: 3,
    skillId: options.skillId ?? 'model_cox',
    args: options.args ?? { time: 'fu_pt', event: 'death', predictors: ['age'] },
    roles: options.roles,
    now: () => new Date('2026-07-13T00:00:00.000Z'),
  });
}

function codes(result: ReturnType<typeof audit>): string[] {
  return result.findings.map((finding) => finding.code);
}

describe('auditDataset', () => {
  it('blocks duplicate keys, invalid chronology, nonpositive person-time, and invalid events', () => {
    const result = audit([
      'participant_id,start_date,exposure_date,end_date,fu_pt,death,age',
      'P001,2026-01-10,2026-02-01,2026-01-09,0,2,50',
      'P001,not-a-date,2026-01-01,2026-03-01,1,1,51',
    ].join('\n'), {
      roles: {
        primary_key: ['participant_id'],
        time_zero: 'start_date',
        exposure_time: 'exposure_date',
        follow_up_end: 'end_date',
        person_time: 'fu_pt',
        event: 'death',
      },
    });

    expect(result.status).toBe('blocked');
    expect(codes(result)).toEqual(expect.arrayContaining([
      'DUPLICATE_PRIMARY_KEY',
      'TIME_VALUE_INVALID',
      'TIME_ORDER_INVALID',
      'IMMORTAL_TIME_RISK',
      'PERSON_TIME_NONPOSITIVE',
      'EVENT_ENCODING_INVALID',
    ]));
    expect(result.findings.every((finding) => finding.sample_row_numbers.length <= 5)).toBe(true);
    expect(JSON.stringify(result)).not.toContain('P001');
  });

  it('blocks unsupported survey, cluster, paired, and repeated-measure designs', () => {
    const result = audit([
      'participant_id,visit,survey_weight,psu,cluster_id,pair_id,fu_pt,death,age',
      'P001,1,1.2,10,A,M1,1,0,50',
      'P001,1,1.1,10,A,M1,2,1,51',
    ].join('\n'), {
      skillId: 'model_linear',
      args: { outcome: 'age', predictors: ['visit'] },
      roles: {
        primary_key: ['participant_id'],
        weight: 'survey_weight',
        psu: 'psu',
        cluster: 'cluster_id',
        pair_id: 'pair_id',
        repeat_index: 'visit',
      },
    });

    expect(result.status).toBe('blocked');
    expect(codes(result)).toEqual(expect.arrayContaining([
      'DUPLICATE_PRIMARY_KEY',
      'DUPLICATE_OBSERVATION_KEY',
      'SURVEY_DESIGN_UNSUPPORTED',
      'CLUSTERING_UNSUPPORTED',
      'PAIRED_REPEATED_UNSUPPORTED',
    ]));
  });

  it('blocks direct identifiers and unresolved clustering clues', () => {
    const result = audit([
      'participant_id,phone,site,fu_pt,death,age',
      'P001,13800000000,A,1,0,50',
      'P002,13900000000,B,2,1,51',
    ].join('\n'));

    expect(result.status).toBe('blocked');
    expect(codes(result)).toEqual(expect.arrayContaining([
      'SENSITIVE_FIELD_PRESENT',
      'POSSIBLE_CLUSTERING',
    ]));
    expect(codes(result)).not.toContain('DUPLICATE_PRIMARY_KEY');
  });

  it('blocks common clinical aliases instead of passing an apparently clean schema', () => {
    const result = audit([
      'participant_id,patient_name,baseline_dt,exposure_dt,exit_dt,iptw,clinic,wave,y,x',
      'P001,Alice Smith,2026-02-30,2026-03-01,2026-01-01,1.2,A,1,1,2',
      'P002,Bob Jones,2026-01-01,2026-01-02,2025-12-31,0.8,B,2,2,3',
    ].join('\n'), {
      skillId: 'model_linear',
      args: { outcome: 'y', predictors: ['x'] },
    });

    expect(result.status).toBe('blocked');
    expect(codes(result)).toEqual(expect.arrayContaining([
      'SENSITIVE_FIELD_PRESENT',
      'TIME_VALUE_INVALID',
      'TIME_ORDER_INVALID',
      'IMMORTAL_TIME_RISK',
      'SURVEY_DESIGN_UNSUPPORTED',
      'POSSIBLE_CLUSTERING',
      'PAIRED_REPEATED_UNSUPPORTED',
    ]));
    expect(result.roles).toMatchObject({
      primary_key: ['participant_id'],
      time_zero: 'baseline_dt',
      exposure_time: 'exposure_dt',
      follow_up_end: 'exit_dt',
      weight: 'iptw',
      repeat_index: 'wave',
    });
    expect(JSON.stringify(result)).not.toContain('Alice Smith');
  });

  it('allows a clean keyed dataset and versions the audit rules', () => {
    const result = audit([
      'participant_id,fu_pt,death,age',
      'P001,1,0,50',
      'P002,2,1,51',
    ].join('\n'));

    expect(result.status).toBe('passed');
    expect(result.findings).toEqual([]);
    expect(result.audit_rules_version).toBe('1.2.0');
    expect(result.audit_sha256).toMatch(/^[0-9a-f]{64}$/);
  });

  it('adopts declared roles that exist, flags missing ones, and keys the union gate (D11)', () => {
    const result = audit([
      'participant_id,start_date,end_date,fu_pt,death,x,ok',
      'P001,2026-02-30,2026-03-01,0,2,1,0',
      'P001,2026-01-01,2026-03-01,1,1,2,1',
    ].join('\n'), {
      roles: {
        primary_key: ['x'],
        time_zero: 'nope',
        person_time: 'nope',
        event: 'ok',
      },
    });

    expect(result.status).toBe('blocked');
    expect(codes(result)).toEqual(expect.arrayContaining([
      // 'nope' does not exist → the declaration is refused, inference stays.
      'AUDIT_ROLE_COLUMN_MISSING',
      // Adopted key `x` is unique, but the inferred participant_id is
      // duplicated — the union gate still surfaces it.
      'DUPLICATE_PRIMARY_KEY',
      'TIME_VALUE_INVALID',
      'PERSON_TIME_NONPOSITIVE',
    ]));
    expect(codes(result)).not.toContain('AUDIT_ROLE_OVERRIDE_REJECTED');
    // Adopted event `ok` is validly 0/1-coded, so the death-column encoding
    // blocker no longer applies.
    expect(codes(result)).not.toContain('EVENT_ENCODING_INVALID');
    expect(result.roles).toMatchObject({ primary_key: ['x'], time_zero: 'start_date', person_time: 'fu_pt', event: 'ok' });
  });

  it('lets an outcome column named weight pass without a survey-design blocker (D12)', () => {
    const result = audit([
      'participant_id,weight,group',
      'P001,4.17,ctrl',
      'P002,5.58,trt',
      'P003,5.18,ctrl',
    ].join('\n'), {
      skillId: 'anova',
      args: { group: 'group', testVar: 'weight' },
    });

    expect(codes(result)).not.toContain('SURVEY_DESIGN_UNSUPPORTED');
    expect(result.status).toBe('passed');
  });

  it('still blocks survey designs when the weight role is explicitly declared (D12)', () => {
    const result = audit([
      'participant_id,weight,group',
      'P001,1.2,ctrl',
      'P002,0.8,trt',
    ].join('\n'), {
      skillId: 'anova',
      args: { group: 'group', testVar: 'weight' },
      roles: { weight: 'weight' },
    });

    expect(result.status).toBe('blocked');
    expect(codes(result)).toContain('SURVEY_DESIGN_UNSUPPORTED');
  });

  it('unblocks a whitelist-free dataset once the user declares its primary key (D11)', () => {
    const csv = [
      'rownames,mpg,cyl',
      'Mazda RX4,21,6',
      'Datsun 710,22.8,4',
    ].join('\n');

    const unresolved = audit(csv, { skillId: 'correlation', args: { x: 'mpg', y: 'cyl' } });
    expect(codes(unresolved)).toContain('PRIMARY_KEY_UNBOUND');

    const declared = audit(csv, {
      skillId: 'correlation',
      args: { x: 'mpg', y: 'cyl' },
      roles: { primary_key: ['rownames'] },
    });
    expect(declared.status).toBe('passed');
    expect(declared.roles).toMatchObject({ primary_key: ['rownames'] });
  });

  it('adopted primary keys are still held to the uniqueness gate (D11)', () => {
    const result = audit([
      'rownames,grp,mpg,cyl',
      'A,g1,21,6',
      'B,g1,22.8,4',
    ].join('\n'), {
      skillId: 'correlation',
      args: { x: 'mpg', y: 'cyl' },
      roles: { primary_key: ['grp'] },
    });

    expect(result.status).toBe('blocked');
    expect(codes(result)).toContain('DUPLICATE_PRIMARY_KEY');
  });

  it('blocks missing analysis values instead of letting the runner coerce them to zero', () => {
    const result = audit([
      'participant_id,fu_pt,death,age',
      'P001,,0,50',
      'P002,2,,51',
      'P003,3,1,52',
    ].join('\n'));

    expect(result.status).toBe('blocked');
    expect(codes(result)).toContain('ANALYSIS_VALUE_MISSING');
  });

  it('audits both selected correlation variables for missing columns and values', () => {
    const missingColumn = audit([
      'participant_id,x',
      'P001,1',
      'P002,2',
    ].join('\n'), {
      skillId: 'correlation',
      args: { x: 'x', y: 'y' },
    });
    expect(codes(missingColumn)).toContain('ANALYSIS_COLUMN_MISSING');

    const missingValue = audit([
      'participant_id,x,y',
      'P001,1,2',
      'P002,2,',
    ].join('\n'), {
      skillId: 'correlation',
      args: { x: 'x', y: 'y' },
    });
    expect(codes(missingValue)).toContain('ANALYSIS_VALUE_MISSING');
  });

  it('blocks empty or malformed table structure and missing analysis columns', () => {
    const empty = audit('participant_id,fu_pt,death,age\n');
    expect(codes(empty)).toEqual(expect.arrayContaining(['DATASET_NO_ROWS', 'EVENT_NO_VARIATION']));

    const malformed = audit('participant_id,,age,age\nP001,1,2');
    expect(codes(malformed)).toEqual(expect.arrayContaining([
      'HEADER_INVALID',
      'ROW_WIDTH_MISMATCH',
      'ANALYSIS_COLUMN_MISSING',
    ]));
  });

  it('blocks missing and unresolved primary keys', () => {
    const missing = audit([
      'participant_id,fu_pt,death,age',
      ',1,0,50',
      'P002,2,1,51',
    ].join('\n'));
    expect(codes(missing)).toContain('PRIMARY_KEY_MISSING');

    const unresolved = audit([
      'row_label,fu_pt,death,age',
      'A,1,0,50',
      'B,2,1,51',
    ].join('\n'));
    expect(codes(unresolved)).toContain('PRIMARY_KEY_UNBOUND');
  });

  it('blocks a binary event column with no variation', () => {
    const result = audit([
      'participant_id,fu_pt,death,age',
      'P001,1,0,50',
      'P002,2,0,51',
    ].join('\n'));
    expect(codes(result)).toContain('EVENT_NO_VARIATION');
  });
});
