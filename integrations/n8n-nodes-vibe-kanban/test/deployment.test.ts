import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

const packageDir = join(import.meta.dirname, '..');

describe('deployment assets', () => {
  it('keeps the VK package in a custom extension path outside the n8n user volume', () => {
    const dockerfile = readFileSync(join(packageDir, 'Dockerfile.n8n'), 'utf8');

    expect(dockerfile).toContain('FROM docker.n8n.io/n8nio/n8n:${N8N_VERSION}');
    expect(dockerfile).toContain(
      'ENV N8N_CUSTOM_EXTENSIONS=/opt/n8n-custom/node_modules',
    );
    expect(dockerfile).toContain(
      'npm install --omit=dev /tmp/n8n-nodes-vibe-kanban-*.tgz',
    );
  });

  it('ships a compose example that builds the custom n8n image', () => {
    const composeExample = readFileSync(
      join(packageDir, 'compose.example.yml'),
      'utf8',
    );

    expect(composeExample).toContain(
      'dockerfile: integrations/n8n-nodes-vibe-kanban/Dockerfile.n8n',
    );
    expect(composeExample).toContain('- n8n_data:/home/node/.n8n');
  });
});
