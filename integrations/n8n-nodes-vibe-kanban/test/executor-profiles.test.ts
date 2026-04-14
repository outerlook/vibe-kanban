import { describe, expect, it } from 'vitest';

import {
  buildExecutorProfileId,
  parseExecutorConfigs,
  toExecutorOptions,
  toExecutorVariantOptions,
} from '../nodes/VibeKanban/shared/executor-profiles';

const rawProfiles = JSON.stringify({
  executors: {
    CLAUDE_CODE: {
      DEFAULT: {
        CLAUDE_CODE: {
          dangerously_skip_permissions: true,
        },
      },
      PLAN: {
        CLAUDE_CODE: {
          plan: true,
        },
      },
    },
    CODEX: {
      DEFAULT: {
        CODEX: {
          model: 'gpt-5.2-codex',
        },
      },
      APPROVALS: {
        CODEX: {
          ask_for_approval: 'unless-trusted',
        },
      },
      HIGH: {
        CODEX: {
          model_reasoning_effort: 'high',
        },
      },
    },
  },
});

describe('executor profile helpers', () => {
  it('lists executors with readable labels', () => {
    const options = toExecutorOptions(parseExecutorConfigs(rawProfiles));

    expect(options).toEqual([
      { name: 'Claude Code', value: 'CLAUDE_CODE' },
      { name: 'Codex', value: 'CODEX' },
    ]);
  });

  it('lists variants with default first for the selected executor', () => {
    const options = toExecutorVariantOptions(
      parseExecutorConfigs(rawProfiles),
      'CODEX',
    );

    expect(options).toEqual([
      { name: 'Default', value: '' },
      { name: 'Approvals', value: 'APPROVALS' },
      { name: 'High', value: 'HIGH' },
    ]);
  });

  it('builds executor profile ids with null default variant', () => {
    expect(buildExecutorProfileId('', '')).toBeNull();
    expect(buildExecutorProfileId('CODEX', '')).toEqual({
      executor: 'CODEX',
      variant: null,
    });
    expect(buildExecutorProfileId('CODEX', 'HIGH')).toEqual({
      executor: 'CODEX',
      variant: 'HIGH',
    });
  });
});
