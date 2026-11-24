#!/bin/sh
# NOT_RUN: live paid remote GPU, full Unity/Unreal/Godot 4.7.2 rendering, AW-033:C MCP.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd -P)
if [ -n "${ASSETWEFT_BIN:-}" ]; then
    bin="$ASSETWEFT_BIN"
else
    target_dir=${CARGO_TARGET_DIR:-}
    if [ -z "$target_dir" ]; then
        metadata=$(CDPATH= cd -- "$root" && cargo metadata --offline --format-version 1 --no-deps)
        target_dir=$(printf '%s' "$metadata" | node -p 'JSON.parse(require("node:fs").readFileSync(0)).target_directory')
    fi
    bin="$target_dir/debug/assetweft"
fi
bin=$(node -p 'require("node:path").resolve(process.argv[1])' "$bin")
[ -x "$bin" ] || { printf 'Build the assetweft CLI first: %s\n' "$bin" >&2; exit 1; }
sha256=$(node -p 'require("node:crypto").createHash("sha256").update(require("node:fs").readFileSync(process.argv[1])).digest("hex")' "$bin")
printf 'assetweft_bin=%s\nassetweft_sha256=%s\n' "$bin" "$sha256"
tmp_parent=$(CDPATH= cd -- "${TMPDIR:-/tmp}" && pwd -P)
tmp=$(mktemp -d "$tmp_parent/assetweft-extended-recipe.XXXXXX")
trap 'rm -rf -- "$tmp"' 0
trap 'exit 1' HUP INT TERM

cd "$tmp"
umask 077
mkdir home tmp original receipts
export HOME="$tmp/home" TMPDIR="$tmp/tmp"
unset HF_TOKEN KAGGLE_API_TOKEN KAGGLE_USERNAME KAGGLE_KEY KAGGLE_CONFIG_DIR
cp "$root/crates/assetweft-core/tests/fixtures/validation/mesh_only.glb" seed.glb
cat > adapter <<'ADAPTER'
#!/bin/sh
printf 'invoked\n' >> "$ASSETWEFT_RECIPE_INVOCATIONS"
exit 1
ADAPTER
chmod +x adapter
export ASSETWEFT_BYO_GPU_ADAPTER="$tmp/adapter" ASSETWEFT_RECIPE_INVOCATIONS="$tmp/invocations"

asset=recipe
version=recipe-v1
digest=$(node -e '
const fs = require("node:fs");
const hash = require("node:crypto").createHash("sha256").update(fs.readFileSync("seed.glb")).digest("hex");
fs.writeFileSync("version.json", JSON.stringify({
  asset_id: "recipe", version_id: "recipe-v1", content_hash: hash,
  input_id: "base-fixture", source: "DECLARED", authorization: "UNVERIFIED",
  authorized: false, license_id: "UNKNOWN", usage_scope: "TEST_ONLY",
  source_format: "GLB", contract_version: "1.0.0", presence: "UNKNOWN"
}));
console.log(hash);
')
bytes=$(wc -c < seed.glb | tr -d ' ')
db="$tmp/original/library.sqlite"
object="$tmp/original/objects/$digest"

cli() {
    id=$1 expected_exit=$2 expected_status=$3
    shift 3
    command=$1
    case "$command" in library|catalog) command="$1 $2";; esac
    [ ! -e "receipts/$id.json" ]
    rc=0
    "$bin" --call-id "$id" "$@" > "receipts/$id.json" || rc=$?
    cat "receipts/$id.json"
    if [ "$rc" -ne "$expected_exit" ]; then
        printf '%s: expected exit %s, got %s\n' "$id" "$expected_exit" "$rc" >&2
        exit 1
    fi
    node - "$id" "$command" "$expected_status" <<'NODE'
const assert = require('node:assert/strict');
const fs = require('node:fs');
const [id, command, status] = process.argv.slice(2);
const raw = fs.readFileSync(`receipts/${id}.json`, 'utf8');
assert.match(raw, /^[^\r\n]+\n$/);
const receipt = JSON.parse(raw);
assert.equal(receipt.schema_version, 1);
assert.equal(receipt.call_id, id);
assert.equal(receipt.command, command);
assert.equal(receipt.status, status);
if (status === 'PASS') {
  assert.deepEqual(receipt.errors, []);
} else {
  assert.equal(receipt.results, null);
  assert.equal(receipt.errors[0].field_path, 'library');
  assert.match(receipt.errors[0].message, /object hash\/length/);
}
NODE
}

reuse_gate() {
    node - "$1" "$object" "$asset" "$version" "$digest" <<'NODE'
const assert = require('node:assert/strict');
const fs = require('node:fs');
const [prefix, object, asset, version, digest] = process.argv.slice(2);
const result = name => JSON.parse(fs.readFileSync(`receipts/${prefix}-${name}.json`)).results;
const availability = result('availability');
if (availability.availability !== 'AVAILABLE') {
  console.error(`reuse refused: ${availability.version_id} is ${availability.availability}`);
  process.exit(3);
}
assert.equal(availability.version_id, version);
const metadata = result('version').version;
assert.equal(metadata.asset_id, asset);
assert.equal(metadata.version_id, version);
assert.equal(metadata.content_hash, digest);
const {current} = result('current');
// A directly committed version can exist before the asset has a candidate head.
if (current !== null) {
  assert.equal(current.asset_id, asset);
  assert.equal(current.version_id, version);
}
assert.equal(require('node:crypto').createHash('sha256').update(fs.readFileSync(object)).digest('hex'), digest);
NODE
}

check_resume() {
    node - "$digest" <<'NODE'
const assert = require('node:assert/strict');
const fs = require('node:fs');
const receipts = fs.readdirSync('receipts').map(file => JSON.parse(fs.readFileSync('receipts/' + file)));
const result = id => receipts.find(receipt => receipt.call_id === id).results;
assert.equal(receipts.filter(receipt => receipt.command === 'catalog commit-object').length, 1);
assert(!receipts.some(receipt => receipt.command === 'byo-gpu run'));
assert.equal(result('seed').content_hash, process.argv[2]);
assert.equal(result('validate-existing').role.satisfied, true);
assert.equal(result('validate-existing').file_sha256, process.argv[2]);
for (const field of ['version', 'current', 'availability', 'feedback']) {
  assert.deepEqual(result('after-' + field), result('before-' + field));
}
assert.deepEqual(result('backup').manifest, result('restore').manifest);
const before = result('before-feedback').feedback;
const after = result('resumed-feedback').feedback;
assert.deepEqual(before, result('review').feedback);
assert.deepEqual(after, result('resume').feedback);
assert.deepEqual(after, {...before, sequence: before.sequence + 1, feedback_id: 'review-2',
  reason: 'Reviewed after restore'});
NODE
}

sed -n '/^```sh extended-recipe$/,/^```$/p' "$root/skills/assetweft-cli/SKILL.md" | sed '1d;$d' > recipe.sh
[ -s recipe.sh ]
. ./recipe.sh
