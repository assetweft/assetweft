#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
bin=$(node -p 'require("node:path").resolve(process.argv[1])' "${ASSETWEFT_BIN:-$root/target/debug/assetweft}")
[ -x "$bin" ] || { printf 'Build the assetweft CLI first: %s\n' "$bin" >&2; exit 1; }
command -v sqlite3 >/dev/null
tmp=$(mktemp -d "${TMPDIR:-/tmp}/assetweft-recipe.XXXXXX")
trap 'rm -rf -- "$tmp"' EXIT
trap 'exit 1' HUP INT TERM

cd "$tmp"
umask 077
mkdir home tmp bundle out receipts
export HOME="$tmp/home" TMPDIR="$tmp/tmp"
unset HF_TOKEN KAGGLE_API_TOKEN KAGGLE_USERNAME KAGGLE_KEY KAGGLE_CONFIG_DIR ASSETWEFT_BYO_GPU_ADAPTER
cp "$root/crates/assetweft-core/tests/fixtures/validation/mesh_only.glb" base.glb

node - "$root" <<'NODE'
const assert = require('node:assert/strict');
const fs = require('node:fs');
const fixtures = require(process.argv[2] + '/crates/assetweft-core/tests/fixtures/generation_spec/cases.json');
const fixture = fixtures.cases.find(({id}) => id === 'valid_single_motion_increment');
assert(fixture);
const write = (file, obj) => fs.writeFileSync(file, JSON.stringify(obj));
write('request.json', fixture.request);
write('context.json', fixture.context);
write('bad-request.json', {...fixture.request, locked_components: []});
const cap = {id: 'approved', unit: 'Task', capacity: 1, approval: 'ExplicitBounded', source: 'Offline recipe operator declaration'};
write('allowance.json', cap);
write('zero.json', {...cap, id: 'zero', capacity: 0});
write('steps.json', [{id: 'admit', dependencies: []}]);
write('bundle/kernel-metadata.json', {
  id: 'fixture/private-recipe', title: 'Offline recipe', code_file: 'main.py',
  language: 'python', kernel_type: 'script', is_private: true, enable_gpu: true,
  machine_shape: 'NvidiaL4'
});
fs.writeFileSync('bundle/main.py', '# Offline metadata validation only\n');
fs.writeFileSync('input.png', Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aCZkAAAAASUVORK5CYII=', 'base64'));
NODE

cat > adapter <<'SH'
#!/bin/sh
set -eu
printf 'invoked\n' >> invocations
[ "$#" -eq 8 ]
[ "$1 $2 $3 $4 $5 $7" = '--profile hf-zerogpu --model trellis2 --out-dir --image' ]
cmp "$8" input.png
cp base.glb "$6/artifact.glb"
SH
chmod +x adapter

cli() {
    id=$1 expected_exit=$2 expected_status=$3
    shift 3
    command=$1
    case "$command" in library|workflow|budget|catalog|byo-gpu) command="$1 $2";; esac
    case "$id" in
        no-approval|cap-exhausted|no-upload)
            sqlite3 -readonly library.sqlite .dump > before.sql ;;
    esac
    rc=0
    "$bin" --call-id "$id" "$@" > "receipts/$id.json" || rc=$?
    printf '%s exit=%s ' "$id" "$rc"
    cat "receipts/$id.json"
    [ "$rc" -eq "$expected_exit" ] || exit 1
    node - "$id" "$expected_status" "$command" <<'NODE'
const assert = require('node:assert/strict');
const fs = require('node:fs');
const [id, status, command] = process.argv.slice(2);
const raw = fs.readFileSync(`receipts/${id}.json`, 'utf8');
assert(/^[^\r\n]+\n$/.test(raw), 'expected exactly one newline-terminated JSON line');
const r = JSON.parse(raw);
assert.equal(r.schema_version, 1);
assert.equal(r.command, command);
assert.equal(r.call_id, id);
assert.equal(r.status, status);
assert(Array.isArray(r.errors));
assert(Object.hasOwn(r, 'results'));
const v = r.results;
if (status === 'PASS' || status === 'DECLARATION_CONSISTENT') assert.deepEqual(r.errors, []);
switch (id) {
  case 'input-validate':
  case 'output-validate': assert.equal(v.role.satisfied, true); break;
  case 'input-inspect': assert(v.geometry.primitives.length > 0); break;
  case 'plan': assert.equal(v.execution, 'NOT_RUN'); break;
  case 'profiles': assert(v.profiles.some(p => p.id === 'hf-zerogpu' && p.models.some(m => m.id === 'trellis2'))); break;
  case 'bundle-check': assert.equal(v.is_private, true); assert.equal(v.credential_present, false); break;
  case 'no-approval':
  case 'cap-exhausted':
  case 'no-upload':
    assert.equal(r.errors[0].field_path, id === 'no-upload' ? 'allow-remote-upload' : 'budget');
    assert.equal(v, null);
    break;
  case 'generate':
  case 'replay':
    assert.equal(v.state, 'SUCCEEDED');
    assert.equal(v.invoked, id === 'generate');
    assert.equal(v.unit, 'Task');
    assert.equal(v.cash_cost, null);
    assert.equal(v.artifact.file, 'artifact.glb');
    assert.match(v.artifact.sha256, /^[a-f0-9]{64}$/);
    break;
  case 'workflow-claim': assert.equal(v.step_id, 'admit'); break;
  case 'workflow-pending':
  case 'workflow-running':
  case 'workflow-settled':
    assert.equal(v.cancellation_requested, false);
    assert.equal(v.steps.length, 1);
    assert.equal(v.steps[0].id, 'admit');
    assert.equal(v.steps[0].state, id === 'workflow-settled' ? 'succeeded' : id.split('-')[1]);
    if (id === 'workflow-settled') {
      assert.equal(v.steps[0].raw, fs.readFileSync('receipts/output-validate.json', 'utf8').trimEnd());
      assert.equal(v.steps[0].evidence, 'out/artifact.glb');
    }
    break;
  case 'cas-read': assert.equal(v.availability, 'AVAILABLE'); break;
  case 'balance': assert.equal(v.allowance.capacity, 1); assert.equal(v.reserved, 0); assert.equal(v.spent, 1); break;
}
NODE
    case "$id" in
        no-approval|cap-exhausted|no-upload)
            sqlite3 -readonly library.sqlite .dump > after.sql
            cmp before.sql after.sql
            [ ! -e invocations ] && [ -z "$(find out tmp -mindepth 1 -print)" ] || exit 1
            printf 'PASS %s invocations=0 sqlite=unchanged output_and_staging=empty\n' "$id" ;;
    esac
}

prepare_artifact() {
    bytes=$(node <<'NODE'
const fs = require('node:fs');
const crypto = require('node:crypto');
const artifact = fs.readFileSync('out/artifact.glb');
const raw = fs.readFileSync('receipts/output-validate.json', 'utf8').trimEnd();
const sha256 = crypto.createHash('sha256').update(artifact).digest('hex');
require('node:assert/strict').equal(JSON.parse(raw).results.file_sha256, sha256);
fs.writeFileSync('outcome.json', JSON.stringify({state: 'succeeded', raw, evidence: 'out/artifact.glb'}));
fs.writeFileSync('version.json', JSON.stringify({
  asset_id: 'recipe', version_id: 'recipe-v1',
  content_hash: sha256,
  input_id: 'base-fixture', source: 'DECLARED', authorization: 'UNVERIFIED',
  authorized: false, license_id: 'UNKNOWN', usage_scope: 'TEST_ONLY', source_format: 'GLB',
  contract_version: '1.0.0', presence: 'UNKNOWN'
}));
console.log(artifact.length);
NODE
    )
}

sed -n '/^```sh recipe$/,/^```$/p' "$root/skills/assetweft-cli/SKILL.md" | sed '1d;$d' > recipe.sh
[ -s recipe.sh ]
. ./recipe.sh

node <<'NODE'
const assert = require('node:assert/strict');
const fs = require('node:fs');
const result = id => JSON.parse(fs.readFileSync(`receipts/${id}.json`)).results;
const run = result('generate');
assert.deepEqual(result('replay'), {...run, invoked: false});
assert.equal(fs.readFileSync('invocations', 'utf8'), 'invoked\n');
const prior = result('receipt-first');
assert.deepEqual(result('receipt-replay'), prior);
assert.equal(prior.receipts.length, 1);
const {sha256} = run.artifact;
assert.deepEqual(prior.receipts[0].resolution, {Confirmed: {charge: 1}});
assert.deepEqual(prior.receipts[0].usage, [{source: 'artifact-sha256:' + sha256, unit: 'Task', amount: 1}]);
const expected = JSON.parse(fs.readFileSync('version.json'));
assert.equal(expected.content_hash, sha256);
assert.equal(result('cas-commit').content_hash, sha256);
assert.equal(result('cas-commit').byte_length, fs.statSync('out/artifact.glb').size);
assert.equal(result('catalog-version').version.content_hash, sha256);
assert.deepEqual(fs.readFileSync('objects/' + sha256), fs.readFileSync('out/artifact.glb'));
console.log(`PASS replay invocations=1 receipt=identical cas_sha256=${sha256}`);
NODE

state=$(sqlite3 -readonly library.sqlite "
SELECT id, capacity, reserved, spent FROM aw_budget_accounts ORDER BY id;
SELECT id, state, charge FROM aw_budget_intents;
SELECT call_id FROM aw_budget_receipts;
SELECT run_id, id, state, evidence FROM aw_workflow_steps;
SELECT v.version_id, o.byte_length FROM aw_managed_versions v JOIN aw_objects o USING(content_hash);")
[ "$state" = "approved|1|0|1
zero|0|0|0
once|CONFIRMED|1
once
recipe|admit|SUCCEEDED|out/artifact.glb
recipe-v1|$bytes" ]
printf 'PASS SQLite persisted rows\n%s\n' "$state"
