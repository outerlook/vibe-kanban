import { afterEach, describe, expect, it } from 'bun:test';

import { VkRuntimeClient } from '../src/vk/runtime-client';
import { EXECUTION_HISTORY_RECAP_ENTRY_BUDGET } from '../src/vk/types';

const originalFetch = globalThis.fetch;

function envelopeResponse(body: unknown): Response {
  return new Response(
    JSON.stringify({
      success: true,
      data: body,
    }),
    {
      status: 200,
      headers: {
        'content-type': 'application/json',
      },
    },
  );
}

function makeEntry(index: number) {
  return {
    entry_index: index,
    entry: {
      timestamp: null,
      entry_type: { type: 'assistant_message' as const },
      content: `entry-${index}`,
      metadata: null,
    },
  };
}

afterEach(() => {
  globalThis.fetch = originalFetch;
});

describe('VkRuntimeClient.getExecutionNormalizedEntriesForRecap', () => {
  it('orders multi-page history from oldest to newest and exhausts cursors', async () => {
    const requestedBeforeIndexes = [] as Array<string | null>;

    globalThis.fetch = (async (input) => {
      const url = new URL(String(input));
      requestedBeforeIndexes.push(url.searchParams.get('before_index'));

      expect(url.pathname).toBe('/api/execution-processes/exec-1/normalized-entries');
      expect(url.searchParams.get('limit')).toBe('500');

      const beforeIndex = url.searchParams.get('before_index');
      if (beforeIndex === null) {
        return envelopeResponse({
          entries: [makeEntry(2), makeEntry(3)],
          next_before_index: 2,
          has_more: true,
        });
      }

      if (beforeIndex === '2') {
        return envelopeResponse({
          entries: [makeEntry(0), makeEntry(1)],
          next_before_index: null,
          has_more: false,
        });
      }

      throw new Error(`Unexpected before_index ${beforeIndex}`);
    }) as typeof fetch;

    const client = new VkRuntimeClient({
      baseUrl: 'https://vk.example.test',
      authMode: 'none',
    });

    const history = await client.getExecutionNormalizedEntriesForRecap('exec-1');

    expect(requestedBeforeIndexes).toEqual([null, '2']);
    expect(history.entries.map((entry) => entry.entry_index)).toEqual([0, 1, 2, 3]);
    expect(history.totalEntries).toBe(4);
    expect(history.droppedEntries).toBe(0);
    expect(history.truncated).toBe(false);
    expect(history.budget).toEqual({
      maxEntries: EXECUTION_HISTORY_RECAP_ENTRY_BUDGET,
      truncation: 'drop_oldest',
    });
  });

  it('drops the oldest entries when history exceeds the recap budget', async () => {
    globalThis.fetch = (async (input) => {
      const url = new URL(String(input));
      expect(url.searchParams.get('limit')).toBe('500');

      const overBudget = EXECUTION_HISTORY_RECAP_ENTRY_BUDGET + 2;
      return envelopeResponse({
        entries: Array.from({ length: overBudget }, (_, index) => makeEntry(index)),
        next_before_index: null,
        has_more: false,
      });
    }) as typeof fetch;

    const client = new VkRuntimeClient({
      baseUrl: 'https://vk.example.test',
      authMode: 'none',
    });

    const history = await client.getExecutionNormalizedEntriesForRecap('exec-2');

    expect(history.totalEntries).toBe(EXECUTION_HISTORY_RECAP_ENTRY_BUDGET + 2);
    expect(history.truncated).toBe(true);
    expect(history.droppedEntries).toBe(2);
    expect(history.entries).toHaveLength(EXECUTION_HISTORY_RECAP_ENTRY_BUDGET);
    expect(history.entries[0]?.entry_index).toBe(2);
    expect(history.entries.at(-1)?.entry_index).toBe(
      EXECUTION_HISTORY_RECAP_ENTRY_BUDGET + 1,
    );
  });
});
