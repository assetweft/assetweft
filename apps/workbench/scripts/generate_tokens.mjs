import { readFileSync, writeFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

const source = new URL('../tokens/tokens.json', import.meta.url);
export const output = new URL('../src/styles/tokens.css', import.meta.url);

export function generateTokens() {
  const { colors, values } = JSON.parse(readFileSync(source, 'utf8'));
  const entries = Object.entries(colors);
  const declarations = (lines) => lines.map((line) => `  ${line}\n`).join('');
  const theme = (name) => entries.map(([key]) => `--aw-${key}: var(--aw-${name}-${key});`);
  return ':root {\n' + declarations([
    'color-scheme: dark light;',
    ...entries.flatMap(([key, { dark, light }]) => [
      `--aw-dark-${key}: ${dark};`, `--aw-light-${key}: ${light};`,
    ]),
    ...theme('dark'),
    ...Object.entries(values).map(([key, value]) => `--aw-${key}: ${value};`),
  ]) + '}\n'
    + '@media (prefers-color-scheme: light) {\n  :root {\n'
    + declarations(theme('light')).replace(/^  /gm, '    ').trimEnd() + '\n  }\n}\n'
    + '@supports (color: light-dark(white, black)) {\n  :root {\n'
    + declarations(entries.map(([key]) =>
      `--aw-${key}: light-dark(var(--aw-light-${key}), var(--aw-dark-${key}));`,
    )).replace(/^  /gm, '    ').trimEnd() + '\n  }\n}\n';
}

if (process.argv[1] && pathToFileURL(process.argv[1]).href === import.meta.url) {
  writeFileSync(output, generateTokens());
}
