import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

const examplesDir = join(import.meta.dirname, '..', 'examples');

describe('workflow examples', () => {
  it('ships first-party examples for the cutover flows', () => {
    const exampleFiles = readdirSync(examplesDir)
      .filter((file) => file.endsWith('.workflow.json'))
      .sort();

    expect(exampleFiles).toEqual(
      expect.arrayContaining([
        'approval-router.workflow.json',
        'autopilot-continuation.workflow.json',
        'coderabbit-review-extraction.workflow.json',
        'feedback-collection.workflow.json',
        'generate-and-merge-follow-up.workflow.json',
        'review-attention.workflow.json',
      ])
    );
  });

  it('keeps VK-backed examples on the VK n8n contract', () => {
    for (const file of readdirSync(examplesDir).filter((entry) =>
      entry.endsWith('.workflow.json')
    )) {
      const workflow = JSON.parse(
        readFileSync(join(examplesDir, file), 'utf8')
      ) as {
        nodes: Array<{ type: string; parameters?: Record<string, unknown> }>;
      };

      expect(workflow.nodes.length).toBeGreaterThan(0);

      const vkNodes = workflow.nodes.filter((node) =>
        node.type.startsWith('n8n-nodes-vibe-kanban.')
      );
      if (vkNodes.length === 0) {
        continue;
      }

      for (const node of vkNodes) {
        if (node.type === 'n8n-nodes-vibe-kanban.vibeKanbanTrigger') {
          expect(node.parameters?.schemaVersion).toBe('vk_n8n_orchestration_v1');
        }
      }
    }
  });

  it('routes review attention through an AI review step fed by the coding turn', () => {
    const workflow = JSON.parse(
      readFileSync(join(examplesDir, 'review-attention.workflow.json'), 'utf8'),
    ) as {
      nodes: Array<{ type: string; parameters?: Record<string, unknown> }>;
    };

    expect(
      workflow.nodes.some(
        (node) => node.type === '@n8n/n8n-nodes-langchain.chainLlm',
      ),
    ).toBe(true);

    const serialized = JSON.stringify(workflow);
    expect(serialized).toContain('coding_agent_turn.summary');
    expect(serialized).toContain('coding_agent_turn.prompt');
    expect(serialized).not.toContain('pending_questions.length > 0');
  });

  it('polls open PRs and keeps only unresolved CodeRabbit review threads', () => {
    const workflow = JSON.parse(
      readFileSync(
        join(examplesDir, 'coderabbit-review-extraction.workflow.json'),
        'utf8',
      ),
    ) as {
      nodes: Array<{ type: string; parameters?: Record<string, unknown> }>;
    };

    expect(
      workflow.nodes.some(
        (node) => node.type === 'n8n-nodes-base.scheduleTrigger',
      ),
    ).toBe(true);

    expect(
      workflow.nodes.some((node) => node.type === 'n8n-nodes-base.code'),
    ).toBe(true);

    const serialized = JSON.stringify(workflow);
    expect(serialized).toContain('/graphql');
    expect(serialized).toContain('reviewThreads(first: 100');
    expect(serialized).toContain('isResolved');
    expect(serialized).toContain("state: 'open'");
    expect(serialized).toContain('coderabbitai[bot]');
    expect(serialized).toContain('/api/projects/');
    expect(serialized).toContain('/github-repositories');
    expect(serialized).toContain('/workflow-association');
    expect(serialized).toContain('$getWorkflowStaticData');
    expect(serialized).toContain('processedThreadCommentIds');
  });
});
