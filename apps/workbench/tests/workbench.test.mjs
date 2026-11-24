import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
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
const { WorkbenchController } = await import(
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

test('tokens are byte-identical and component CSS and DOM construction pass the gate', () => {
  const output = execFileSync(process.execPath, ['scripts/check_tokens.mjs'], { cwd: project, encoding: 'utf8' });
  assert.match(output, /token diff = 0/);
});
