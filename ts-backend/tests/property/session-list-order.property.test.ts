// tests/property/session-list-order.property.test.ts — Property 8.
//
// For an arbitrary set of sessions with arbitrary last_active_at timestamps,
// MemSessionStore.list() returns summaries sorted by last_active_at in
// non-increasing (descending) order.
//
// Validates: Requirements 11.2

import { describe, it, expect } from 'vitest';
import fc from 'fast-check';
import { MemSessionStore } from '@stats-code/server';

describe('Property 8: session list descending order (Requirement 11.2)', () => {
  it('list() is sorted by last_active_at non-increasing', async () => {
    await fc.assert(
      fc.asyncProperty(
        // A set of distinct epoch-millis timestamps to assign to sessions.
        fc.array(fc.integer({ min: 0, max: 4_000_000_000_000 }), { minLength: 0, maxLength: 12 }),
        async (timestamps) => {
          const store = new MemSessionStore();
          for (const ms of timestamps) {
            const s = await store.create();
            // Mutate the stored entity's last_active_at (same reference).
            s.last_active_at = new Date(ms).toISOString();
          }
          const list = await store.list();
          expect(list).toHaveLength(timestamps.length);
          for (let i = 1; i < list.length; i++) {
            // Non-increasing: previous >= current.
            expect(
              list[i - 1]!.last_active_at.localeCompare(list[i]!.last_active_at),
            ).toBeGreaterThanOrEqual(0);
          }
        },
      ),
      { numRuns: 50 },
    );
  });
});
