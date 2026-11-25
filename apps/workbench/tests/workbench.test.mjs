import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';
import { build } from 'esbuild';

const project = fileURLToPath(new URL('../', import.meta.url));
const { outputFiles } = await build({
  absWorkingDir: project,
  entryPoints: ['src/controller.ts'],
  bundle: true,
  write: false,
  format: 'esm',
  platform: 'node',
});
const { WorkbenchController, errorMessage } = await import(
  `data:text/javascript;base64,${Buffer.from(outputFiles[0].text).toString('base64')}`
);

const session = { dbPath: '/test/catalog.sqlite', schemaVersion: 6, mode: 'LOCAL_SQLITE_NO_LOGIN', capabilities: [] };
const row = (versionId, availability = 'AVAILABLE') => ({
  assetId: 'example', versionId, sourceFormat: 'GLB', availability, latestDecision: 'UNRATED',
});
const versions = (matches) => ({ matches, truncated: false });

function harness(overrides = {}) {
  const state = { errors: [] };
  const adapter = {
    bootstrap: async () => null,
    openLibrary: async () => session,
    listVersions: async () => versions([]),
    getVersionDetail: async (versionId) => ({ versionId, history: [] }),
    loadPreview: async (versionId) => ({ versionId, kind: 'GLB' }),
    ...overrides,
  };
  const view = {
    session(value) { state.session = value; },
    list(value, query) { Object.assign(state, { list: value, query }); },
    selection(value) { Object.assign(state, { selection: value, detail: null, preview: null }); },
    detail(value) { state.detail = value; },
    preview(value) { state.preview = value; },
    validation(value) { state.validation = value; },
    budget(value) { state.budget = value; },
    candidates(value) { state.candidates = value; },
    capabilities(value) { state.capabilities = value; },
    run(value) { state.run = value; },
    attempt(value) { state.attempt = value; },
    budgetReceipt() {},
    initializedHead() {},
    proposedCandidate() {},
    error(area, message) { state.errors.push({ area, message }); },
  };
  return { controller: new WorkbenchController(adapter, view), state };
}

test('startup with no library preserves an unavailable catalog', async () => {
  const { controller, state } = harness({
    listVersions: async () => assert.fail('Listing before open'),
  });
  await controller.start();
  assert.equal(state.session, null);
  assert.equal(state.list, null);
  assert.equal(state.selection, null);
  assert.deepEqual(state.errors, []);
});

test('open loads the default list and forwards searches to Core', async () => {
  const queries = [];
  const { controller, state } = harness({
    listVersions: async (query, limit) => {
      queries.push([query, limit]);
      return versions(query ? [row('mesh')] : []);
    },
  });
  await controller.open(session.dbPath, true);
  assert.deepEqual(state.session, session);
  assert.deepEqual(state.list, versions([]));
  await controller.search('mesh');
  assert.deepEqual(state.list, versions([row('mesh')]));
  assert.deepEqual(queries, [['', 100], ['mesh', 100]]);
});

test('older detail and GLB IPC replies cannot replace a newer unavailable selection', async () => {
  const delayedPreview = Promise.withResolvers();
  const delayedDetail = Promise.withResolvers();
  const { controller, state } = harness({
    getVersionDetail: async (id) => id === 'old' ? delayedDetail.promise : { versionId: id },
    loadPreview: async (id) => id === 'old' ? delayedPreview.promise : { versionId: id, kind: 'UNAVAILABLE' },
  });
  await controller.open(session.dbPath, true);
  const old = controller.select(row('old'));
  await controller.select(row('new', 'METADATA_ONLY'));
  delayedPreview.resolve({ kind: 'GLB', versionId: 'old' });
  delayedDetail.resolve({ versionId: 'old' });
  await old;
  assert.equal(state.selection.versionId, 'new');
  assert.equal(state.detail.versionId, 'new');
  assert.deepEqual(state.preview, { versionId: 'new', kind: 'UNAVAILABLE' });
});

test('feedback refreshes the exact-version history and visible list badges', async () => {
  let record;
  const { controller, state } = harness({
    listVersions: async () => versions([{ ...row('mesh'), latestDecision: record?.decision ?? 'UNRATED' }]),
    appendFeedback: async (request) => (record = { sequence: 1, ...request }),
    getVersionDetail: async (versionId) => ({ versionId, history: record ? [record] : [] }),
  });
  await controller.open(session.dbPath, true);
  await controller.select(row('mesh'));
  await controller.feedback({ assetId: 'example', versionId: 'mesh', decision: 'ACCEPT', feedbackId: 'f1', reason: null });
  assert.deepEqual(state.detail.history, [record]);
  assert.equal(state.list.matches[0].latestDecision, 'ACCEPT');
  assert.equal(state.preview.versionId, 'mesh');
});

test('preserves Core errors and does not report rejected feedback as saved', async () => {
  const err = { code: 'INVALID_INPUT', message: 'Invalid feedback identity' };
  const { controller, state } = harness({
    loadPreview: async () => { throw { code: 'CORRUPT', message: 'Hash mismatch' }; },
    appendFeedback: async () => { throw err; },
    openLibrary: async (path) => {
      if (!path) throw err;
      return session;
    },
  });
  await assert.rejects(controller.open('', false), (error) => error === err);
  assert.equal(state.session, undefined);
  await controller.open(session.dbPath, false);
  await controller.select(row('mesh'));
  assert.equal(state.preview, null);
  assert.deepEqual(state.errors, [{ area: 'preview', message: 'CORRUPT — Hash mismatch' }]);
  await assert.rejects(controller.feedback({ assetId: 'example', versionId: 'mesh', decision: 'ACCEPT', feedbackId: '', reason: null }),
    (error) => error === err);
  assert.deepEqual(state.detail.history, []);
});

test('committed tokens, component CSS and DOM construction pass the gate', () => {
  const output = execFileSync(process.execPath, ['scripts/check_tokens.mjs'], { cwd: project, encoding: 'utf8' });
  assert.match(output, /token diff = 0/);
});

test('token gate accepts CRLF and rejects a changed token value', (t) => {
  const fixture = mkdtempSync(join(tmpdir(), 'aw-token-gate-'));
  t.after(() => rmSync(fixture, { recursive: true, force: true }));
  for (const directory of ['scripts', 'src', 'tokens']) {
    cpSync(join(project, directory), join(fixture, directory), { recursive: true });
  }

  const stylesheet = join(fixture, 'src/styles/tokens.css');
  const crlf = readFileSync(stylesheet, 'utf8').replace(/\r?\n/g, '\r\n');
  const args = ['scripts/check_tokens.mjs'];
  const options = { cwd: fixture, encoding: 'utf8' };
  writeFileSync(stylesheet, crlf);
  const green = spawnSync(process.execPath, args, options);
  assert.equal(green.status, 0, green.stderr);
  assert.match(green.stdout, /token diff = 0/);

  const edited = crlf.replace('--aw-space-4: 4px;', '--aw-space-4: 5px;');
  assert.notEqual(edited, crlf);
  writeFileSync(stylesheet, edited);
  const red = spawnSync(process.execPath, args, options);
  assert.equal(red.status, 1, red.stderr);
  assert.match(red.stderr, /Regenerate src\/styles\/tokens\.css/);
  writeFileSync(stylesheet, crlf);

  const css = join(fixture, 'src/styles/components.css');
  const original = readFileSync(css, 'utf8');
  writeFileSync(css, original + '\n.invalid { color: #abc; }');
  const colorRed = spawnSync(process.execPath, args, options);
  assert.equal(colorRed.status, 1, colorRed.stderr);
  assert.match(colorRed.stderr, /color/i);
  writeFileSync(css, original);

  writeFileSync(join(fixture, 'src/invalid.ts'), "document.body.innerHTML = '';");
  const domRed = spawnSync(process.execPath, args, options);
  assert.equal(domRed.status, 1, domRed.stderr);
  assert.match(domRed.stderr, /innerHTML/);
});

test('budget overview preserves UNKNOWN and refreshes after local cancel and reconciliation', async () => {
  let intent = { id: 'i', state: 'UNKNOWN', quote: null, charge: null, cancelled: false, warning: 'do NOT duplicate-submit' };
  const { controller, state } = harness({
    budgetOverview: async (limit) => { assert.equal(limit, 100); return { intents: [intent] }; },
    cancelCall: async (id) => {
      assert.equal(id, 'i');
      return (intent = { ...intent, cancelled: true, warning: 'Local cancel recorded; remote stop or refund is NOT claimed.' });
    },
    reconcileCall: async (req) => {
      assert.deepEqual(req.resolution, { Confirmed: { charge: 1 } });
      return (intent = { ...intent, charge: 1, state: 'CONFIRMED' });
    },
  });
  await controller.open(session.dbPath, false);
  await controller.setMode('budget');
  assert.equal(state.budget.intents[0].quote, null);
  assert.match(state.budget.intents[0].warning, /duplicate-submit/);
  await controller.budgetAction((bridge) => bridge.cancelCall('i'));
  assert.equal(state.budget.intents[0].cancelled, true);
  assert.equal(state.budget.intents[0].state, 'UNKNOWN');
  await controller.budgetAction((bridge) => bridge.reconcileCall({ resolution: { Confirmed: { charge: 1 } } }));
  assert.equal(state.budget.intents[0].state, 'CONFIRMED');
  assert.equal(state.budget.intents[0].charge, 1);
});

test('BYO GPU keeps upload refusal and renders the settled replay receipt', async () => {
  const error = { code: 'INVALID_INPUT', message: 'requires --allow-remote-upload' };
  const calls = [];
  const { controller, state } = harness({
    budgetOverview: async () => ({ intents: [] }),
    runByoGpu: async (req) => {
      calls.push(req.allowRemoteUpload);
      if (!req.allowRemoteUpload) throw error;
      return { status: 'SUCCEEDED', invoked: false, cashCost: null, message: 'Settled receipt replayed' };
    },
  });
  await controller.open(session.dbPath, false);
  await controller.setMode('budget');
  const run = (allowRemoteUpload) => controller.runByoGpu({ image: '/fixture.png', allowRemoteUpload });
  await assert.rejects(run(false), (err) => err === error);
  assert.equal(state.run, null);
  await run(true);
  assert.deepEqual(calls, [false, true]);
  assert.equal(state.run.invoked, false);
  assert.equal(state.run.cashCost, null);
  assert.match(state.run.message, /replayed/);
});

test('candidate proposal and accept refresh the head while conflict preserves the stale branch', async () => {
  let heads = [];
  let candidates = [];
  const { controller, state } = harness({
    candidateOverview: async () => ({ heads, candidates }),
    proposeCandidate: async (req) => {
      const candidate = { ...req, attempt: null };
      candidates = [...candidates, candidate];
      return candidate;
    },
    attemptAcceptCandidate: async (id) => {
      const decision = id === 'a' ? 'ACCEPTED' : 'CONFLICT';
      if (decision === 'ACCEPTED') heads = [{ assetId: 'asset', versionId: 'next', revision: 1 }];
      const attempt = { candidateId: id, decision, currentRevision: 1 };
      candidates = candidates.map((item) => item.id === id ? { ...item, attempt } : item);
      return attempt;
    },
  });
  await controller.open(session.dbPath, false);
  await controller.setMode('candidates');
  await controller.proposeCandidate({ id: 'a', candidateVersion: 'next', expectedRevision: 0 });
  await controller.attemptCandidate('a');
  assert.equal(state.attempt.decision, 'ACCEPTED');
  await controller.proposeCandidate({ id: 'b', candidateVersion: 'stale', expectedRevision: 0 });
  await controller.attemptCandidate('b');
  assert.equal(state.attempt.decision, 'CONFLICT');
  assert.equal(state.candidates.heads[0].versionId, 'next');
  assert.equal(state.candidates.heads[0].revision, 1);
  assert.equal(state.candidates.candidates[1].candidateVersion, 'stale');
  assert.equal(state.candidates.candidates[1].attempt.decision, 'CONFLICT');
});

test('old workspace results cannot paint after a tab change or library switch', async () => {
  const budget = Promise.withResolvers();
  const candidate = Promise.withResolvers();
  let current = session.dbPath;
  const { controller, state } = harness({
    openLibrary: async (dbPath) => { current = dbPath; return { ...session, dbPath }; },
    budgetOverview: () => budget.promise,
    candidateOverview: async () => current === session.dbPath ? candidate.promise : { heads: [], candidates: [] },
  });
  await controller.open(session.dbPath, false);
  const oldBudget = controller.setMode('budget');
  const oldCandidate = controller.setMode('candidates');
  await controller.open('/new.sqlite', false);
  budget.resolve({ intents: [{ id: 'old' }] });
  candidate.resolve({ heads: [{ assetId: 'old' }] });
  await Promise.all([oldBudget, oldCandidate]);
  assert.equal(state.budget, null);
  assert.deepEqual(state.candidates, { heads: [], candidates: [] });
});

test('capabilities load without a library and run only registered inputs', async () => {
  const overview = {
    implemented: [
      { id: 'motion-retarget', implemented: true, allowedParams: ['primaryPath', 'maxBytes'] },
      { id: 'sprite-inspect', implemented: true, secondaryRequired: true, allowedParams: ['primaryPath', 'secondaryPath', 'maxBytes'] },
    ],
    unimplemented: [{ id: 'unity_live_import', implemented: false, status: 'NOT_RUN', allowedParams: [] }],
  };
  const calls = [];
  const { controller, state } = harness({
    capabilityOverview: async () => overview,
    runCapabilityCheck: async (request) => {
      calls.push(request);
      if (request.capabilityId === 'sprite-inspect') {
        throw { code: 'UNSUPPORTED', message: 'Runtime NOT_RUN' };
      }
      return { status: 'PASS', notRun: ['visual_engine_status: NOT_RUN'] };
    },
  });
  await controller.start();
  await controller.setMode('capabilities');
  assert.equal(state.session, null);
  assert.deepEqual(state.capabilities, overview);
  const request = { capabilityId: 'motion-retarget', primaryPath: ' /tmp/retarget.json ', maxBytes: 1024 };
  await assert.rejects(controller.runCapabilityCheck({ ...request, secondaryPath: '/tmp/injected.json' }), /does not accept/);
  await assert.rejects(controller.runCapabilityCheck({ ...request, maxBytes: 0 }), /whole number/);
  await assert.rejects(controller.runCapabilityCheck({ ...request, capabilityId: 'sprite-inspect' }), /secondary file path/);
  await assert.rejects(controller.runCapabilityCheck({ ...request, capabilityId: 'unity_live_import' }),
    (error) => errorMessage(error).startsWith('UNSUPPORTED — Capability is NOT_RUN'));
  assert.deepEqual(calls, []);

  assert.deepEqual(await controller.runCapabilityCheck(request), { status: 'PASS', notRun: ['visual_engine_status: NOT_RUN'] });
  assert.deepEqual(calls, [{ ...request, primaryPath: '/tmp/retarget.json' }]);
  await assert.rejects(controller.runCapabilityCheck({
    ...request, capabilityId: 'sprite-inspect', primaryPath: '/tmp/sheet.png', secondaryPath: '/tmp/sprite.json',
  }), (error) => errorMessage(error) === 'UNSUPPORTED — Runtime NOT_RUN');
});
