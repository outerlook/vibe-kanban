import { defineConfig } from '@trigger.dev/sdk';

export default defineConfig({
  project: process.env.TRIGGER_PROJECT_REF ?? 'vk_trigger_project_ref',
  runtime: 'bun',
  maxDuration: 300,
  dirs: ['./src/trigger'],
});
