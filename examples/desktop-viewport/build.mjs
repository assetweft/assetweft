import {build} from 'esbuild';
import {mkdir,copyFile} from 'node:fs/promises';
await mkdir('dist',{recursive:true});
await build({entryPoints:['src/main.ts'],bundle:true,format:'esm',outfile:'dist/main.js',target:'safari15'});
await copyFile('index.html','dist/index.html');
