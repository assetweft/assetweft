#!/bin/sh
# Offline AW-035:A fixture verifier. It never contacts a GPU provider.
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
BIN=${ASSETWEFT_BIN:-"$ROOT/target/debug/assetweft"}
TRELLIS=
BLENDER=
GODOT=
while [ "$#" -gt 0 ]; do
    case "$1" in
        --trellis-glb) TRELLIS=$2; shift 2 ;;
        --blender) BLENDER=$2; shift 2 ;;
        --godot) GODOT=$2; shift 2 ;;
        *) echo "usage: $0 [--trellis-glb FILE] [--blender BIN] [--godot BIN]" >&2; exit 2 ;;
    esac
done
[ -x "$BIN" ] || { echo "ASSETWEFT_BIN is not executable: $BIN" >&2; exit 2; }
command -v node >/dev/null || { echo "node is required" >&2; exit 2; }
command -v sqlite3 >/dev/null || { echo "sqlite3 is required" >&2; exit 2; }

TMP=$(mktemp -d "${TMPDIR:-/tmp}/assetweft-aw035a.XXXXXX")
trap 'rm -rf "$TMP"' EXIT HUP INT TERM
DB=$TMP/library.sqlite

expect() {
    wanted_code=$1 wanted_status=$2
    shift 2
    set +e
    reply=$("$@")
    code=$?
    set -e
    printf '%s\n' "$reply"
    printf '%s' "$reply" | ACTUAL="$code" node -e '
const [code, status] = process.argv.slice(1); let v;
try { v = JSON.parse(require("fs").readFileSync(0, "utf8")); } catch (e) { process.exitCode = 1; throw e; }
if (Number(code) !== Number(process.env.ACTUAL) || v.status !== status) {
  console.error(JSON.stringify({expected:{code:Number(code),status},actual:{code:Number(process.env.ACTUAL),status:v.status}})); process.exit(1);
}' "$wanted_code" "$wanted_status"
    [ "$code" -eq "$wanted_code" ]
    REPLY=$reply
}

result() {
    printf '%s' "$REPLY" | node -e 'const v=JSON.parse(require("fs").readFileSync(0, "utf8")); eval(process.argv[1])' "$1"
}

sha256() { node -e 'const fs=require("fs"),c=require("crypto"); console.log(c.createHash("sha256").update(fs.readFileSync(process.argv[1])).digest("hex"))' "$1"; }

write_version() {
    node -e 'const fs=require("fs"),c=require("crypto"); const [out,id,asset,source,input,format,file]=process.argv.slice(1); const b=fs.readFileSync(file); fs.writeFileSync(out, JSON.stringify({asset_id:asset,version_id:id,content_hash:c.createHash("sha256").update(b).digest("hex"),input_id:input,source,authorization:"local AW-035:A fixture declaration",authorized:true,license_id:"CC-BY-4.0",usage_scope:"LOCAL_SAMPLE",source_format:format,contract_version:"1.0.0",presence:"UNKNOWN"}));' "$TMP/version.json" "$1" "$2" "$3" "$4" "$5" "$6"
}

commit() {
    write_version "$1" "$2" "$3" "$4" "$5" "$6"
    expect 0 PASS "$BIN" catalog commit-object --db "$DB" --version "$TMP/version.json" --file "$6" --expected-len "$(wc -c < "$6" | tr -d ' ')"
}

reference() {
    node -e 'const fs=require("fs"); fs.writeFileSync(process.argv[1], JSON.stringify({owner_version_id:process.argv[2],role:process.argv[3],component_version_id:process.argv[4]}));' "$TMP/reference.json" "$1" "$2" "$3"
    expect 0 PASS "$BIN" catalog add-reference --db "$DB" --reference "$TMP/reference.json"
}

expect 0 PASS "$BIN" library init --db "$DB"
printf '%s' '{"id":"local-task","unit":"Task","capacity":1,"approval":"ExplicitBounded","source":"AW-035:A local fixture only"}' > "$TMP/allowance.json"
expect 0 PASS "$BIN" budget approve --db "$DB" --allowance "$TMP/allowance.json"

# Build a minimal static mesh with a required embedded WebP texture when no real GLB is supplied.
STATIC=$TMP/static-webp.glb
if [ -n "$TRELLIS" ]; then
    cp "$TRELLIS" "$STATIC"
else
    node -e '
const fs=require("fs"), src=fs.readFileSync(process.argv[1]);
const jsonEnd=20+src.readUInt32LE(12), doc=JSON.parse(src.subarray(20,jsonEnd));
let bin=Buffer.from(src.subarray(jsonEnd+8)); const pixel=Buffer.from("524946461a000000574542505650384c0e0000002f00000000071011fd0f4444ff03","hex");
doc.extensionsUsed=["EXT_texture_webp"]; doc.extensionsRequired=["EXT_texture_webp"];
doc.buffers[0].byteLength=bin.length+pixel.length; doc.bufferViews.push({buffer:0,byteOffset:bin.length,byteLength:pixel.length});
doc.images=[{bufferView:doc.bufferViews.length-1,mimeType:"image/webp"}]; doc.textures=[{extensions:{EXT_texture_webp:{source:0}}}];
let j=Buffer.from(JSON.stringify(doc)); j=Buffer.concat([j,Buffer.alloc((4-j.length%4)%4,0x20)]); bin=Buffer.concat([bin,pixel,Buffer.alloc((4-(bin.length+pixel.length)%4)%4)]);
const out=Buffer.alloc(12); out.write("glTF"); out.writeUInt32LE(2,4); out.writeUInt32LE(28+j.length+bin.length,8);
fs.writeFileSync(process.argv[2],Buffer.concat([out,Buffer.from(Uint32Array.of(j.length).buffer),Buffer.from("JSON"),j,Buffer.from(Uint32Array.of(bin.length).buffer),Buffer.from("BIN\0"),bin]));
' "$ROOT/crates/assetweft-core/tests/fixtures/validation/mesh_only.glb" "$STATIC"
fi
printf 'RIFF\032\000\000\000WEBPVP8L\016\000\000\000/\000\000\000\000\007\020\021\375\017DD\377\003' > "$TMP/input.webp"
cat > "$TMP/adapter.sh" <<'EOF'
#!/bin/sh
set -eu
while [ "$#" -gt 0 ]; do
  if [ "$1" = "--out-dir" ]; then cp "$AW035_STATIC" "$2/artifact.glb"; exit 0; fi
  shift
done
exit 2
EOF
chmod +x "$TMP/adapter.sh"
export AW035_STATIC=$STATIC
expect 0 PASS "$BIN" byo-gpu run --db "$DB" --budget-id local-task --intent-id static-once --profile hf-zerogpu --model trellis2 --out-dir "$TMP/byo" --image "$TMP/input.webp" --allow-remote-upload --adapter-bin "$TMP/adapter.sh"
BYO=$TMP/byo/artifact.glb
expect 0 PASS "$BIN" validate "$BYO" --role static-mesh
expect 0 PASS "$BIN" inspect "$BYO"
result 'if (!v.results.geometry.extensions_required.includes("EXT_texture_webp")) process.exit(1)'
expect 1 FAIL "$BIN" validate "$BYO" --role rigged-character
commit static static-mesh UNKNOWN trellis2 GLB "$BYO"
expect 0 PASS "$BIN" catalog file-availability --db "$DB" --version-id static

SOURCE=$ROOT/examples/rigged-simple/RiggedSimple.glb
ORIGINAL=$TMP/original-components
expect 0 PASS "$BIN" rigged-simple-components "$SOURCE" --out-dir "$ORIGINAL"
commit original character UNKNOWN Cesium-RiggedSimple GLB "$SOURCE"
for role in mesh materials rig motion; do
    commit "$role" "$role" UNKNOWN original JSON "$ORIGINAL/$role.json"
    reference original "$role" "$role"
done
printf '%s' '{"contract_id":"RIGGED_SIMPLE_MOTION_STRUCTURE_V1","max_component_bytes":16777216,"required_quality":[]}' > "$TMP/contract.json"
expect 0 PASS "$BIN" candidate initialize --db "$DB" --version-id original --contract "$TMP/contract.json"
printf '%s' '{"id":"edit","base_version":"original","candidate_version":"edited","expected_revision":0,"editable":["motion"],"lock_mode":"BYTE","contract":{"contract_id":"RIGGED_SIMPLE_MOTION_STRUCTURE_V1","max_component_bytes":16777216,"required_quality":[]}}' > "$TMP/proposal.json"
expect 0 PASS "$BIN" candidate schedule-rigged-simple-motion --db "$DB" --proposal "$TMP/proposal.json"
expect 0 PASS "$BIN" workflow status --db "$DB" --run-id edit
result 'if (JSON.stringify(v.results.steps.map(x=>x.id)) !== "[\"motion\"]" || v.results.steps[0].state !== "pending") process.exit(1)'
expect 0 PASS "$BIN" workflow claim --db "$DB" --run-id edit
result 'if (v.results.step_id !== "motion") process.exit(1)'
expect 0 PASS "$BIN" catalog materialize-export --db "$DB" --version-id original
EXPORTED_ORIGINAL=$(result 'process.stdout.write(v.results.relative_path)')
STRIKE=$TMP/strike.glb
expect 0 PASS "$BIN" motion-strike "$TMP/$EXPORTED_ORIGINAL" --with-idle --out "$STRIKE"
[ "$(sha256 "$STRIKE")" = 667d8bfccdeb68d64c9668ff54ab0af907aa240988a3d488f49ff7b43152b09d ]
printf '%s' '{"state":"succeeded","raw":"local motion-strike fixture"}' > "$TMP/outcome.json"
expect 0 PASS "$BIN" workflow finish --db "$DB" --run-id edit --step-id motion --outcome "$TMP/outcome.json"
EDITED=$TMP/edited-components
expect 0 PASS "$BIN" rigged-simple-components "$STRIKE" --out-dir "$EDITED"
commit edited character PROCEDURAL original GLB "$STRIKE"
commit edited-motion motion PROCEDURAL original JSON "$EDITED/motion.json"
reference edited mesh mesh
reference edited materials materials
reference edited rig rig
reference edited motion edited-motion
expect 0 PASS "$BIN" candidate propose-rigged-simple-motion --db "$DB" --proposal "$TMP/proposal.json"
expect 0 PASS "$BIN" candidate attempt --db "$DB" --candidate-id edit
result 'const a=v.results.receipt.Accepted; if (!a || a.current.version_id!=="edited" || a.current.revision!==1 || a.report.components.length!==2 || a.report.components.some(x=>x.requirement!=="ANIMATED_CHARACTER"||x.format_status!=="PASS"||x.role_status!=="PASS")) process.exit(1)'
expect 0 PASS "$BIN" catalog materialize-export --db "$DB" --version-id edited
EXPORTED_EDITED=$(result 'process.stdout.write(v.results.relative_path)')
cmp "$STRIKE" "$TMP/$EXPORTED_EDITED"

cp "$EDITED/materials.json" "$TMP/changed-material.json"
printf ' ' >> "$TMP/changed-material.json"
commit changed character PROCEDURAL edited GLB "$STRIKE"
commit changed-material materials PROCEDURAL edited JSON "$TMP/changed-material.json"
reference changed mesh mesh
reference changed materials changed-material
reference changed rig rig
reference changed motion edited-motion
printf '%s' '{"id":"changed","base_version":"edited","candidate_version":"changed","expected_revision":1,"editable":["motion"],"lock_mode":"BYTE"}' > "$TMP/changed.json"
expect 2 INVALID_INPUT "$BIN" candidate propose-rigged-simple-motion --db "$DB" --proposal "$TMP/changed.json"
printf '%s' '{"id":"stale","base_version":"original","candidate_version":"edited","expected_revision":0,"editable":["motion"],"lock_mode":"BYTE","contract":{"contract_id":"RIGGED_SIMPLE_MOTION_STRUCTURE_V1","max_component_bytes":16777216,"required_quality":[]}}' > "$TMP/stale.json"
expect 0 PASS "$BIN" candidate propose-rigged-simple-motion --db "$DB" --proposal "$TMP/stale.json"
expect 1 FAIL "$BIN" candidate attempt --db "$DB" --candidate-id stale
result 'if (v.errors[0].code !== "CONFLICT" || !v.results.receipt.Conflict) process.exit(1)'
expect 0 PASS "$BIN" catalog current --db "$DB" --asset-id character
result 'if (v.results.current.version_id !== "edited" || v.results.current.revision !== 1) process.exit(1)'

if [ -n "$BLENDER" ]; then
    expect 0 PASS "$BIN" rig-bend "$SOURCE" --blender "$BLENDER" --out "$TMP/rig-bend"
    "$BLENDER" --background --factory-startup --disable-autoexec --python-exit-code 1 --python "$ROOT/examples/rigged-simple/playback.py" -- "$SOURCE" "$TMP/$EXPORTED_EDITED" --with-idle
    echo ENGINE_BLENDER=PASS
else
    echo ENGINE_BLENDER=NOT_RUN
fi
if [ -n "$GODOT" ]; then
    GD=$TMP/godot
    mkdir "$GD"
    cp "$ROOT/examples/godot-static/project.godot" "$ROOT/examples/rigged-simple/godot_playback.gd" "$ROOT/examples/rigged-simple/godot_binding_control.gd" "$TMP/$EXPORTED_EDITED" "$GD/"
    (cd "$GD" && "$GODOT" --headless --path . --script godot_playback.gd -- "$(basename "$EXPORTED_EDITED")")
    (cd "$GD" && "$GODOT" --headless --path . --script godot_binding_control.gd -- "$(basename "$EXPORTED_EDITED")" normal && "$GODOT" --headless --path . --script godot_playback.gd -- normal.glb)
    set +e; (cd "$GD" && "$GODOT" --headless --path . --script godot_binding_control.gd -- "$(basename "$EXPORTED_EDITED")" root-only && "$GODOT" --headless --path . --script godot_playback.gd -- root-only.glb); code=$?; set -e
    [ "$code" -ne 0 ]
    echo ENGINE_GODOT=PASS
else
    echo ENGINE_GODOT=NOT_RUN
fi
echo LIVE_PAID_GPU=NOT_RUN
echo AW035A=PASS
