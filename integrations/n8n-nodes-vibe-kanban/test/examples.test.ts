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
        'feedback-collection.workflow.json',
        'generate-and-merge-follow-up.workflow.json',
        'review-attention.workflow.json',
      ])
    );
  });

  it('keeps every example on the VK n8n contract', () => {
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
      expect(vkNodes.length).toBeGreaterThan(0);

      for (const node of vkNodes) {
        if (node.type === 'n8n-nodes-vibe-kanban.vibeKanbanTrigger') {
          expect(node.parameters?.schemaVersion).toBe('vk_n8n_orchestration_v1');
        }
      }
    }
  });
});
