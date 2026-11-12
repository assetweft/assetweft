// Responsibility: keep the AW-003 invalid-manifest regressions in the required CI.
// Dependencies: Node standard library; the versioned contract checker and fixtures.
// Caller: npm test and the contracts workflow. These tests execute no assets.
// Invariants: invalid test selection is INVALID_INPUT, never an empty/partial PASS;
// only each test's own temporary directory is removed; product fixtures stay unchanged.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const original = JSON.parse(fs.readFileSync(path.join(root, 'contracts/fixtures/manifest.json'), 'utf8'));
const cases = [
  ['empty selection', (m) => { m.expectations = []; }],
  ['duplicate case', (m) => { m.expectations.push(structuredClone(m.expectations[0])); }],
  ['orphan expectation', (m) => { m.expectations.push({ id: 'TEST_FIXTURE/missing', expect: 'VALID' }); }],
  ['invalid expected outcome', (m) => { m.expectations[0].expect = 'UNDECLARED_OUTCOME'; }],
  ['missing fixture coverage', (m) => { m.expectations.pop(); }],
];

for (const [name, mutate] of cases) {
  test(`invalid manifest: ${name}`, () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'assetweft-envelope-'));
    try {
      const manifest = structuredClone(original);
      mutate(manifest);
      const manifestPath = path.join(dir, 'manifest.json');
      fs.writeFileSync(manifestPath, JSON.stringify(manifest));
      const result = spawnSync(process.execPath, [
        path.join(root, 'tools/contract_check.mjs'), '--manifest', manifestPath,
      ], { cwd: root, encoding: 'utf8', timeout: 10000 });
      assert.ifError(result.error);
      assert.equal(result.signal, null, result.stderr);
      assert.equal(result.status, 2, result.stderr);
      const output = JSON.parse(result.stdout);
      assert.equal(output.result, 'INVALID_INPUT');
      assert.equal(output.gate, 'FAIL');
    } finally {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  });
}
