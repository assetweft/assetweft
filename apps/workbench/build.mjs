import { build } from 'esbuild';
import { copyFile } from 'node:fs/promises';

await build({
  entryPoints: ['src/main.ts'],
  bundle: true,
  format: 'esm',
  outdir: 'dist',
  target: ['safari15', 'chrome105'],
});
await copyFile('index.html', 'dist/index.html');
