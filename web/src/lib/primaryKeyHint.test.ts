import { describe, it, expect } from 'vitest';
import { detectPrimaryKey, PRIMARY_KEY_NAME_EXAMPLES } from './primaryKeyHint';

describe('detectPrimaryKey', () => {
  it('recognises every documented candidate name', () => {
    for (const name of ['participant_id', 'subject_id', 'patient_id', 'person_id', 'record_id', 'study_id', 'case_id']) {
      expect(detectPrimaryKey([name, 'age'])).toEqual({ resolved: true, column: name });
    }
  });

  it('normalises case, spaces, hyphens and dots like the server does', () => {
    // 服务端 normalizeFieldName 把非字母数字片段折成 '_'，这些表头都能识别；
    // 前端漏掉这步会误报「未识别到主键」，比不提示更糟。
    expect(detectPrimaryKey(['Patient ID']).resolved).toBe(true);
    expect(detectPrimaryKey(['patient-id']).resolved).toBe(true);
    expect(detectPrimaryKey(['patient.id']).resolved).toBe(true);
    expect(detectPrimaryKey(['  SUBJECT_ID  ']).resolved).toBe(true);
  });

  it('accepts the *_key pattern variants', () => {
    expect(detectPrimaryKey(['record_key']).resolved).toBe(true);
    expect(detectPrimaryKey(['study_key']).resolved).toBe(true);
  });

  it('reports unresolved when no column looks like an identifier', () => {
    expect(detectPrimaryKey(['age', 'bmi', 'smoke'])).toEqual({ resolved: false, column: null });
  });

  it('reports unresolved for an empty column list', () => {
    expect(detectPrimaryKey([])).toEqual({ resolved: false, column: null });
  });

  it('does not accept a bare id column', () => {
    // 服务端判据不含裸 'id'——前端也不能宽松，否则审批阶段仍会被阻断。
    expect(detectPrimaryKey(['id', 'age']).resolved).toBe(false);
  });

  it('prefers the earliest candidate in the documented priority order', () => {
    // participant_id 在候选表里排第一，应优先于 case_id。
    expect(detectPrimaryKey(['case_id', 'participant_id']).column).toBe('participant_id');
  });

  it('returns the original column spelling, not the normalised form', () => {
    // 提示文案要引用用户表头里的原样拼写。
    expect(detectPrimaryKey(['Patient ID']).column).toBe('Patient ID');
  });

  it('exposes the candidate names for the hint copy', () => {
    expect(PRIMARY_KEY_NAME_EXAMPLES).toContain('participant_id');
    expect(PRIMARY_KEY_NAME_EXAMPLES).toContain('case_id');
  });
});
