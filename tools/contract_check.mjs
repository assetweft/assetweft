#!/usr/bin/env node
// 职责(responsibility): AW-003:A 本地开发期「首条交付合同」文档检查器。先校验 fixtures/manifest 信封
//   （非空、唯一 ID、合法 expect、一一覆盖、TEST_FIXTURE 溯源在外包装），再对每个合同实例做结构校验
//   (Ajv draft-07) + 显式语义校验，并核对实际结果是否符合 manifest 期望。它是合同/文档级 gate，
//   不执行任何生成、引擎导入、GPU 或真实 F0 资产检查；不产出任何资产字节；不读取任何文件字节或凭据。
// 依赖(dependencies): Node.js >=24 标准库 (fs, path, crypto, url)；Ajv 8.17.1 作为 devDependency
//   (JSON Schema draft-07 标准实现)。无网络调用、无产品运行时依赖。
// 调用方(callers): `npm run check`；`node tools/contract_check.mjs --manifest ... --fixtures ... --schema ...`；
//   .github/workflows/contracts.yml（定义已准备，本卡未触发 → NOT_RUN）。
// 不变量(invariants):
//   1. stdout 只输出单个机器可读 JSON；所有日志/诊断走 stderr。
//   2. VALID 仅表示「合同文档通过声明范围内的检查」，≠ 执行 PASS；fixture 一律 metadata_only。
//   3. UNKNOWN/NOT_RUN/INPUT_INCOMPLETE 不得默认成 PASS；PARTIAL 摘要不得 Whole-Mesh 语义 PASS。
//   4. SEMANTIC 锁在规范化器未 IMPLEMENTED 时只能 NOT_RUN；已知规范化器不存在时不自造规范化器。
//   5. 每组件按预声明 lock_mode 比较自身 baseline/after 摘要；不一致 → LOCK_VIOLATION。
//   6. 引擎版本 UNKNOWN 不阻断 S1/S2 文件/契约检查；仅 S3 要求具体版本与 increment。
//   7. 组件 source 必须指向已声明文件，且其 content_hash 必须等于该文件声明的 sha256（不读文件字节）。
//   8. 公共引用最小边界：拒本地绝对/UNC/file/遍历/带凭据 URL；保留 opaque/relative；不造存储协议或通用 secret scanner。
//   9. 测试标记在 fixture 外包装或可选 annotation；schema 不强制产品合同带 fixture 字段。
//  10. 期望值只来自 manifest.json，fixture 文档内不携带期望，避免「把同一生产常量抄进测试后自证」。
//  11. 退出码沿用 docs/CONTRACTS.md §10.5 命名: 0=gate 全匹配; 1=存在不匹配(FAIL);
//      2=输入/信封/schema 不可用(INVALID_INPUT); 6=未预期错误(INTERNAL)。

import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import { fileURLToPath } from 'node:url';
import AjvModule from 'ajv';

const Ajv = AjvModule.default || AjvModule;
const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const SOURCE_PATH = path.join(ROOT, 'tools/contract_check.mjs');
const DEFAULT_SCHEMA = path.join(ROOT, 'contracts/first-delivery-contract.schema.json');
const DEFAULT_FIXTURES = path.join(ROOT, 'contracts/fixtures/fixtures.json');
const DEFAULT_MANIFEST = path.join(ROOT, 'contracts/fixtures/manifest.json');

// docs/CONTRACTS.md §10.5 结果命名。VALID 是文档级通过（§8 机器格式通过与视觉/生成通过分开）。
const OUTCOME = Object.freeze({
  VALID: 'VALID',
  INVALID_INPUT: 'INVALID_INPUT',
  INPUT_INCOMPLETE: 'INPUT_INCOMPLETE',
  LOCK_VIOLATION: 'LOCK_VIOLATION',
  FAIL: 'FAIL',
  UNSUPPORTED: 'UNSUPPORTED',
  INTERNAL: 'INTERNAL',
});
const ALLOWED_EXPECT = new Set(Object.values(OUTCOME));
// 多重违规时取更严重者（数值越大越严重），保证错误语义可预测。
const RANK = Object.freeze({
  VALID: 0, FAIL: 1, UNSUPPORTED: 2, LOCK_VIOLATION: 3,
  INPUT_INCOMPLETE: 4, INVALID_INPUT: 5, INTERNAL: 6,
});

const sha256File = (p) => crypto.createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const sha256Json = (v) => crypto.createHash('sha256').update(JSON.stringify(v)).digest('hex');
const log = (...a) => process.stderr.write('[contract_check] ' + a.join(' ') + '\n');

function parseArgs(argv) {
  const out = { schema: DEFAULT_SCHEMA, fixtures: DEFAULT_FIXTURES, manifest: DEFAULT_MANIFEST };
  for (let i = 0; i < argv.length; i++) {
    const k = argv[i];
    if ((k === '--schema' || k === '--fixtures' || k === '--manifest') && argv[i + 1]) {
      out[k.slice(2)] = path.resolve(ROOT, argv[++i]);
    } else {
      throw new Error('unknown argument: ' + k);
    }
  }
  return out;
}

function readJson(p) {
  try {
    return JSON.parse(fs.readFileSync(p, 'utf8'));
  } catch (e) {
    throw new Error('cannot parse ' + p + ': ' + e.message);
  }
}

// ---- 公共引用最小边界：deny-list，不是通用 secret scanner，不发明存储协议 ----
// 允许：合法 opaque/相对引用与合成 metadata:// fixture。拒绝：本地绝对 Unix/Windows/UNC、
// file:、路径遍历、URL authority 中带 userinfo 的凭据形式。
function unsafePublicRef(r) {
  if (typeof r !== 'string' || r.length === 0) return 'empty reference';
  if (r.startsWith('/')) return 'local absolute Unix path';
  if (/^[A-Za-z]:/.test(r)) return 'drive-qualified / local Windows path';
  const BS = String.fromCharCode(92);
  if (r.startsWith(BS + BS)) return 'UNC path';
  if (r.includes(BS)) return 'backslash path separator (local path)';
  if (/^file:/i.test(r)) return 'file:// URL';
  if (/(^|\/)\.\.(\/|$)/.test(r)) return 'path traversal';
  const m = r.match(/^[A-Za-z][A-Za-z0-9+.-]*:\/\/([^/?#]*)/);
  if (m && m[1].includes('@')) return 'credential-bearing URL (userinfo in authority)';
  return null;
}

// ---- 信封校验：非空、唯一 ID、合法 expect、一一覆盖、TEST_FIXTURE 溯源在外包装 ----
function validateEnvelopes(fixtureSet, manifest) {
  const reasons = [];
  const fixtures = Array.isArray(fixtureSet && fixtureSet.fixtures) ? fixtureSet.fixtures : null;
  const expectations = Array.isArray(manifest && manifest.expectations) ? manifest.expectations : null;
  if (!fixtures || fixtures.length === 0) reasons.push('fixtures.json: fixtures[] must be a non-empty array');
  if (!expectations || expectations.length === 0) reasons.push('manifest.json: expectations[] must be a non-empty array');
  const fIds = [], eIds = [];
  if (fixtures) {
    const seen = new Set();
    fixtures.forEach((f, i) => {
      if (!f || typeof f.id !== 'string' || f.id.length === 0) reasons.push('fixtures[' + i + ']: missing/invalid id');
      else {
        if (seen.has(f.id)) reasons.push('fixtures: duplicate id ' + f.id);
        seen.add(f.id);
        fIds.push(f.id);
      }
      if (!f || !f.document || typeof f.document !== 'object') reasons.push('fixtures[' + i + ']: missing document object');
      const tf = f && f.test_fixture;
      if (!tf || tf.marker !== 'TEST_FIXTURE' || tf.metadata_only !== true)
        reasons.push('fixtures[' + i + ']: test_fixture envelope must carry marker=TEST_FIXTURE and metadata_only=true');
    });
  }
  if (expectations) {
    const seen = new Set();
    expectations.forEach((e, i) => {
      if (!e || typeof e.id !== 'string' || e.id.length === 0) reasons.push('expectations[' + i + ']: missing/invalid id');
      else {
        if (seen.has(e.id)) reasons.push('expectations: duplicate id ' + e.id);
        seen.add(e.id);
        eIds.push(e.id);
      }
      if (!e || !ALLOWED_EXPECT.has(e.expect)) reasons.push('expectations[' + i + ']: invalid expect ' + (e && e.expect));
    });
  }
  if (fixtures && expectations) {
    const fset = new Set(fIds), eset = new Set(eIds);
    for (const id of fset) if (!eset.has(id)) reasons.push('coverage: fixture has no expectation: ' + id);
    for (const id of eset) if (!fset.has(id)) reasons.push('coverage: expectation has no fixture: ' + id);
  }
  return reasons;
}

// ---- 显式语义校验：仅在 Ajv 结构校验通过之后运行 ----
function semanticCheck(doc, envelope) {
  const reasons = [];
  let outcome = OUTCOME.VALID;
  const raise = (next, msg) => {
    reasons.push({ code: next, message: msg });
    if (RANK[next] > RANK[outcome]) outcome = next;
  };
  const info = (msg) => reasons.push({ code: 'INFO', message: msg });

  const stage = doc.stage;
  const ld = doc.lock_declaration || null;
  const inc = doc.increment || null;
  const report = doc.validation_report;

  // R-fixture: 测试溯源在外包装；可选 annotation 必须一致；metadata-only 不得冒充真实授权/known_correct F0
  if (envelope && envelope.metadata_only === true) {
    if (doc.input.authorization.authorized === true)
      raise(OUTCOME.INVALID_INPUT, 'metadata-only fixture must not claim real authorization');
    if (doc.input.known_correct !== null)
      raise(OUTCOME.INVALID_INPUT, 'metadata-only fixture must not claim known_correct F0');
  }
  if (doc.fixture) {
    if (doc.fixture.marker !== 'TEST_FIXTURE' || doc.fixture.metadata_only !== true)
      raise(OUTCOME.INVALID_INPUT, 'optional fixture annotation must carry marker=TEST_FIXTURE and metadata_only=true');
  }

  // R0: 字段按真正消费它的段拆分；缺失只阻断该段及其下游 (§10.3)
  if (stage === 'S2' || stage === 'S3') {
    if (!ld) raise(OUTCOME.INPUT_INCOMPLETE, 'stage ' + stage + ' requires lock_declaration');
  }
  if (stage === 'S3') {
    if (!inc) raise(OUTCOME.INPUT_INCOMPLETE, 'stage S3 requires increment');
    if (report.engine_version === 'UNKNOWN')
      raise(OUTCOME.INPUT_INCOMPLETE, 'stage S3 requires concrete engine_version');
  }
  if (stage === 'S1' && report.engine_version === 'UNKNOWN') {
    info('S1/S2 file-and-contract-only check proceeds with engine_version=UNKNOWN (engine version must not block file/contract checks)');
  }

  // R-public: 公共引用边界（不得暴露磁盘路径/UNC/凭据 URL；保留 opaque/relative）
  const refFields = [];
  for (const f of doc.input.files) refFields.push(['input.files[].path', f.path]);
  for (const c of doc.components) refFields.push(['components[].source', c.source]);
  if (inc) for (const r of inc.input_refs) refFields.push(['increment.input_refs[]', r]);
  for (const r of report.artifact_refs) refFields.push(['validation_report.artifact_refs[]', r]);
  for (const [field, r] of refFields) {
    const bad = unsafePublicRef(r);
    if (bad) raise(OUTCOME.INVALID_INPUT, field + ' rejects ' + bad + ': ' + r);
  }

  // R1: 引用完整性（坏引用）。§2 组件引用、§8 原始产物引用
  const roles = doc.components.map((c) => c.role);
  const rolesSet = new Set(roles);
  if (rolesSet.size !== roles.length) raise(OUTCOME.FAIL, 'duplicate component role');
  const paths = new Set(doc.input.files.map((f) => f.path));
  const fileHash = new Map(doc.input.files.map((f) => [f.path, f.sha256]));
  // 组件 source 必须指向已声明文件，且 content_hash 等于该文件声明的 sha256（不读文件字节）
  for (const c of doc.components) {
    if (!paths.has(c.source)) raise(OUTCOME.FAIL, 'components[].source references undeclared file: ' + c.source);
    else if (fileHash.get(c.source) !== c.content_hash)
      raise(OUTCOME.FAIL, 'components[].content_hash != referenced file declared sha256: ' + c.source);
  }
  if (ld) {
    for (const r of ld.editable_components)
      if (!rolesSet.has(r)) raise(OUTCOME.FAIL, 'editable_components references undeclared role: ' + r);
    for (const l of ld.locked_components)
      if (!rolesSet.has(l.role)) raise(OUTCOME.FAIL, 'locked_components references undeclared role: ' + l.role);
  }
  if (inc) {
    for (const r of inc.input_refs)
      if (!paths.has(r)) raise(OUTCOME.FAIL, 'increment.input_refs references undeclared file: ' + r);
  }
  for (const r of report.artifact_refs)
    if (!paths.has(r)) raise(OUTCOME.FAIL, 'validation_report.artifact_refs references undeclared file: ' + r);

  // R2: 锁定/可编辑范围冲突 (§3 GenerationSpec 可写组件 vs 锁定组件；§10.4 S3)
  if (ld) {
    const locked = new Set(ld.locked_components.map((l) => l.role));
    for (const r of ld.editable_components)
      if (locked.has(r)) raise(OUTCOME.LOCK_VIOLATION, 'role both editable and locked: ' + r);
  }

  // R3: 未知费用不得填零 (§5 费用记录；AGENTS「不把未知费用填零」)
  if (inc) {
    const b = inc.budget;
    if (b.cost_status === 'unknown' && b.cost_value !== null)
      raise(OUTCOME.INVALID_INPUT, 'unknown cost must be null, got ' + JSON.stringify(b.cost_value));
    if (b.cost_status === 'known' && (typeof b.cost_value !== 'number' || b.cost_value < 0))
      raise(OUTCOME.INVALID_INPUT, 'known cost must be a non-negative number');
  }

  // R4: UNKNOWN/NOT_RUN 不得默认 PASS；PARTIAL 摘要不得 Whole-Mesh 语义 PASS (§8, §10.2 规则3)
  for (const it of report.items) {
    if (it.claims_whole_mesh_semantics) {
      if (it.status !== 'PASS' || it.digest_scope !== 'FULL')
        raise(OUTCOME.FAIL, 'claims_whole_mesh_semantics requires status=PASS and digest_scope=FULL (check=' + it.check + ')');
    }
    if (['NOT_RUN', 'INPUT_INCOMPLETE', 'NONDETERMINISTIC', 'WAIVED'].includes(it.status) && it.claims_whole_mesh_semantics)
      raise(OUTCOME.FAIL, 'non-PASS status must not claim whole-mesh semantics (check=' + it.check + ')');
  }

  // R5/R6: 每组件按预声明 lock_mode 比自身前后摘要；未知规范化器只能 NOT_RUN (§10.2 规则2-3, §10.4 S2)
  if (ld) {
    for (const l of ld.locked_components) {
      const items = report.items.filter((i) => i.target === l.role);
      const hasDigests = l.baseline_digest !== null && l.after_digest !== null;
      const normalizerReady = l.lock_mode === 'BYTE' ||
        (l.lock_mode === 'SEMANTIC' && l.canonicalization && l.canonicalization.status === 'IMPLEMENTED');
      let computed;
      if (!hasDigests || !normalizerReady || l.digest_scope !== 'FULL') {
        computed = 'NOT_RUN';
      } else if (l.baseline_digest !== l.after_digest) {
        computed = 'LOCK_VIOLATION';
      } else {
        computed = 'PASS';
      }

      for (const it of items) {
        if (computed === 'NOT_RUN' && it.status === 'PASS')
          raise(OUTCOME.FAIL, 'component ' + l.role + ': report claims PASS but lock comparison must be NOT_RUN (missing digests / unimplemented normalizer / PARTIAL scope)');
      }
      if (computed === 'LOCK_VIOLATION') {
        raise(OUTCOME.LOCK_VIOLATION, 'component ' + l.role + ': baseline_digest != after_digest under lock_mode=' + l.lock_mode);
      }
      if (l.lock_mode === 'SEMANTIC' && (!l.canonicalization || l.canonicalization.status !== 'IMPLEMENTED')) {
        for (const it of items)
          if (it.status === 'PASS')
            raise(OUTCOME.FAIL, 'component ' + l.role + ': SEMANTIC lock with non-IMPLEMENTED canonicalization must be NOT_RUN');
        if (items.length === 0)
          info('component ' + l.role + ': SEMANTIC normalizer not IMPLEMENTED -> lock comparison NOT_RUN (no canonicalizer invented)');
      }
    }
  }

  return { outcome, reasons };
}

function ajvVersion() {
  try {
    return JSON.parse(fs.readFileSync(path.join(ROOT, 'node_modules/ajv/package.json'), 'utf8')).version;
  } catch {
    return 'UNKNOWN';
  }
}

function machineResult(obj, exitCode) {
  process.stdout.write(JSON.stringify(obj, null, 2) + '\n');
  process.exitCode = exitCode;
}

function main() {
  let args;
  try {
    args = parseArgs(process.argv.slice(2));
  } catch (e) {
    log('ERROR ' + e.message);
    machineResult({ tool: 'contract_check.mjs', result: OUTCOME.INVALID_INPUT, error: e.message }, 2);
    return;
  }

  let schema, fixtureSet, manifest;
  try {
    schema = readJson(args.schema);
    fixtureSet = readJson(args.fixtures);
    manifest = readJson(args.manifest);
  } catch (e) {
    log('ERROR ' + e.message);
    machineResult({ tool: 'contract_check.mjs', result: OUTCOME.INVALID_INPUT, error: e.message }, 2);
    return;
  }

  // 信封校验前置于任何 fixture 运行：空/重复/孤儿/非法 expect/缺覆盖一律 INVALID_INPUT(2)，不返回 PASS/0
  const envelopeErrors = validateEnvelopes(fixtureSet, manifest);
  if (envelopeErrors.length) {
    log('ERROR invalid fixture/manifest envelope (' + envelopeErrors.length + ' issue(s))');
    machineResult({
      tool: 'contract_check.mjs',
      tool_version: '1.0.0',
      result: OUTCOME.INVALID_INPUT,
      gate: 'FAIL',
      gate_scope: 'fixture-expectation-match-only; NOT an asset / AI-generation / engine result',
      envelope_errors: envelopeErrors,
      schema_path: path.relative(ROOT, args.schema),
      schema_sha256: sha256File(args.schema),
      source_path: path.relative(ROOT, SOURCE_PATH),
      source_sha256: sha256File(SOURCE_PATH),
      fixtures_path: path.relative(ROOT, args.fixtures),
      fixtures_sha256: sha256File(args.fixtures),
      manifest_path: path.relative(ROOT, args.manifest),
      manifest_sha256: sha256File(args.manifest),
      total: 0, matched: 0, mismatched: 0, results: [],
    }, 2);
    return;
  }

  let validate;
  try {
    // Ajv 是官方 JSON Schema 标准实现；此处不自行重写通用引擎。
    // strict=false 关闭 Ajv 的 schema 书写风格 lint，validateSchema 默认仍校验 schema 文档本身。
    const ajv = new Ajv({ allErrors: true, strict: false, validateSchema: true });
    validate = ajv.compile(schema);
  } catch (e) {
    log('ERROR schema compile failed: ' + e.message);
    machineResult({ tool: 'contract_check.mjs', result: OUTCOME.INVALID_INPUT, error: 'schema compile: ' + e.message }, 2);
    return;
  }

  const entries = new Map(fixtureSet.fixtures.map((f) => [f.id, f]));
  const expectations = manifest.expectations;
  const results = [];
  let mismatched = 0;

  for (const exp of expectations) {
    const entry = entries.get(exp.id);
    const doc = entry && entry.document;
    if (!doc) {
      mismatched++;
      results.push({ id: exp.id, expect: exp.expect, actual: OUTCOME.INTERNAL, matched: false, doc_sha256: null, reasons: [{ code: 'MISSING_FIXTURE', message: 'no document with this id in fixtures.json' }] });
      log('MISMATCH ' + exp.id + ': missing fixture document');
      continue;
    }
    let actual = OUTCOME.VALID;
    let reasons = [];
    try {
      if (!validate(doc)) {
        actual = OUTCOME.INVALID_INPUT;
        reasons = (validate.errors || []).map((e) => ({ code: 'AJV', message: (e.instancePath || '/') + ' ' + e.message }));
      } else {
        const s = semanticCheck(doc, entry.test_fixture);
        actual = s.outcome;
        reasons = s.reasons;
      }
    } catch (e) {
      actual = OUTCOME.INTERNAL;
      reasons = [{ code: 'EXCEPTION', message: e.message }];
      log('EXCEPTION ' + exp.id + ': ' + e.message);
    }
    const matched = actual === exp.expect;
    if (!matched) mismatched++;
    results.push({ id: exp.id, expect: exp.expect, actual, matched, doc_sha256: sha256Json(doc), reasons });
    log((matched ? 'OK ' : 'MISMATCH ') + exp.id + ': expect=' + exp.expect + ' actual=' + actual);
  }

  const gate = mismatched === 0 ? 'PASS' : 'FAIL';
  machineResult({
    tool: 'contract_check.mjs',
    tool_version: '1.0.0',
    gate,
    gate_scope: 'fixture-expectation-match-only; NOT an asset / AI-generation / engine result',
    schema_path: path.relative(ROOT, args.schema),
    schema_sha256: sha256File(args.schema),
    source_path: path.relative(ROOT, SOURCE_PATH),
    source_sha256: sha256File(SOURCE_PATH),
    fixtures_path: path.relative(ROOT, args.fixtures),
    fixtures_sha256: sha256File(args.fixtures),
    manifest_path: path.relative(ROOT, args.manifest),
    manifest_sha256: sha256File(args.manifest),
    ajv_version: ajvVersion(),
    total: expectations.length,
    matched: expectations.length - mismatched,
    mismatched,
    results,
  }, mismatched === 0 ? 0 : 1);
}

try {
  main();
} catch (e) {
  log('INTERNAL ' + (e && e.stack ? e.stack : String(e)));
  machineResult({ tool: 'contract_check.mjs', result: OUTCOME.INTERNAL, error: String(e) }, 6);
}
