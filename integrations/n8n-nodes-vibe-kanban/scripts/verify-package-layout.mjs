import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const packageDir = resolve(import.meta.dirname, '..');
const packageJson = JSON.parse(readFileSync(resolve(packageDir, 'package.json'), 'utf8'));

const missingPaths = [
  ...packageJson.n8n.credentials,
  ...packageJson.n8n.nodes,
].filter((assetPath) => !existsSync(resolve(packageDir, assetPath)));

if (missingPaths.length > 0) {
  console.error('n8n package manifest points to missing build assets:');
  for (const assetPath of missingPaths) {
    console.error(`- ${assetPath}`);
  }
  process.exit(1);
}
