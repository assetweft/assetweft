import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { generateTokens, output } from './generate_tokens.mjs';

assert.equal(readFileSync(output, 'utf8'), generateTokens(), 'Regenerate src/styles/tokens.css');
const src = new URL('../src/', import.meta.url);
for (const file of ['styles/layout.css', 'styles/components.css']) {
  assert.doesNotMatch(readFileSync(new URL(file, src), 'utf8'), /#[\da-f]{3,8}\b|\b(?:rgba?|hsla?)\s*\(/i,
    `${file} must use a semantic color token`);
}
for (const file of readdirSync(src, { recursive: true }).filter((name) => name.endsWith('.ts'))) {
  assert.doesNotMatch(readFileSync(new URL(file.replaceAll('\\', '/'), src), 'utf8'),
    /\b(?:innerHTML|outerHTML|insertAdjacentHTML)\b/, `${file} must use DOM construction`);
}
console.log('token diff = 0, color and DOM checks passed');
