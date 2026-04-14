import { copyFileSync, mkdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const packageDir = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const sourcePath = resolve(packageDir, '..', '..', 'shared', 'types.ts');
const destinationPath = resolve(packageDir, 'generated', 'shared-types.ts');

mkdirSync(dirname(destinationPath), { recursive: true });
copyFileSync(sourcePath, destinationPath);
