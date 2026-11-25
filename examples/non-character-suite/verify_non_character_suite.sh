#!/bin/sh
set -eu

[ "$#" -eq 0 ] || { printf 'usage: %s\n' "$0" >&2; exit 2; }
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
BIN=${ASSETWEFT_BIN:-"$ROOT/target/debug/assetweft"}
[ -x "$BIN" ] || { printf 'ASSETWEFT_BIN is not executable: %s\n' "$BIN" >&2; exit 2; }
for tool in node python3 sqlite3; do
    command -v "$tool" >/dev/null || { printf '%s is required\n' "$tool" >&2; exit 2; }
done

TMP=$(mktemp -d "${TMPDIR:-/tmp}/assetweft-aw035c.XXXXXX")
trap 'rm -rf "$TMP"' EXIT
trap 'exit 1' HUP INT TERM

expect() {
    wanted_code=$1 wanted_status=$2 predicate=$3
    shift 3
    code=0
    reply=$("$BIN" --max-bytes 16777216 "$@") || code=$?
    printf '%s' "$reply" | node -e '
const [actual, expected, status, predicate] = process.argv.slice(1);
const raw = require("fs").readFileSync(0, "utf8");
const v = JSON.parse(raw);
const r = v.results;
if (raw.includes("\n") || actual !== expected || v.schema_version !== 1 || v.status !== status ||
    !Array.isArray(v.errors) || !eval(predicate)) {
    console.error(JSON.stringify({expected, status, predicate, actual, receipt: v}));
    process.exit(1);
}' "$code" "$wanted_code" "$wanted_status" "$predicate"
}

# Canonical paths are required by library backup and restore.
TMP=$(cd "$TMP" && pwd -P)
RIG=$ROOT/examples/rigged-simple/RiggedSimple.glb
STATIC=$ROOT/crates/assetweft-core/tests/fixtures/validation/mesh_only.glb
python3 - "$TMP" "$RIG" "$STATIC" <<'PY'
import hashlib
import json
from pathlib import Path
import struct
import sys
import zlib

tmp, rig, static = map(Path, sys.argv[1:])

def spec(name, value):
    (tmp / (name + ".json")).write_text(json.dumps(value), encoding="utf-8")

def glb(name, doc, chunks):
    data = json.dumps(doc).encode()
    data += b" " * (-len(data) % 4)
    (tmp / (name + ".glb")).write_bytes(
        struct.pack("<4sIII4s", b"glTF", 2, 20 + len(data) + len(chunks), len(data), b"JSON")
        + data + chunks
    )

def chunk(kind, data):
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

def png(name, rows):
    (tmp / (name + ".png")).write_bytes(b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", 2, len(rows), 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(b"".join(b"\0" + bytes(row) for row in rows)))
        + chunk(b"IEND", b""))

png("sheet", [[0, 0, 0, 0, 1, 2, 3, 255], [0, 0, 0, 0, 7, 8, 9, 128]])
png("oak", [[42, 70, 20, 255] * 2])
png("ash", [[99, 60, 40, 255] * 2])
sprite = {"sheet": {"frame_width": 1, "frame_height": 1, "columns": 2, "rows": 2},
          "frames": [{"index": i, "col": i % 2, "row": i // 2,
                      "anchor_x": 0, "anchor_y": 0.5} for i in range(4)],
          "looping": {"loop_cycle": True, "start_frame": 0, "end_frame": 3}}
spec("sprite", sprite)
sprite["looping"]["end_frame"] = 4
spec("sprite-no-loop", sprite)

layout = {"canvas": {"width_px": 60, "height_px": 80},
          "illustration_slot": {"slot_id": "art", "x": 0, "y": 0, "width": 60, "height": 40,
                                "editable_independently": True, "fit_mode": "contain"},
          "text_boxes": [{"box_id": "title", "x": 0, "y": 40, "width": 60, "height": 20,
                          "text": "oak", "font_px": 8, "line_height_px": 10,
                          "max_chars_per_line": 4, "max_lines": 2}]}
for kind in ("ui", "card", "tile"):
    spec(kind, dict(layout, kind=kind))
layout["text_boxes"][0]["text"] = "a\nb\nc"
spec("card-overflow", dict(layout, kind="card"))

for kind in ("architecture", "foliage"):
    closed = kind == "architecture"
    points = [(0, 0, 0), (2, 0, 0), (2, 1, 0), (0, 1, 0)]
    indices = [0, 2, 1, 0, 3, 2]
    if closed:
        points += [(x, y, 0.25) for x, y, _ in points]
        indices += [4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 1, 2, 6, 1, 6, 5,
                    2, 3, 7, 2, 7, 6, 3, 0, 4, 3, 4, 7]
    vertices = b"".join(struct.pack("<3f", *point) for point in points)
    faces = struct.pack(f"<{len(indices)}H", *indices)
    geometry = vertices + faces
    doc = {"asset": {"version": "2.0"}, "scene": 0, "scenes": [{"nodes": [0]}],
           "nodes": [{"translation": [1, 0, 0], "children": [1, 2]},
                     {"mesh": 0}, {"name": "east", "translation": [2, 0, 0]}],
           "meshes": [{"primitives": [{"attributes": {"POSITION": 0}, "indices": 1, "material": 0}]}],
           "materials": [{"alphaMode": "OPAQUE" if closed else "MASK", "doubleSided": not closed}],
           "accessors": [{"bufferView": 0, "componentType": 5126, "count": len(points), "type": "VEC3",
                          "min": [0, 0, 0], "max": [2, 1, 0.25 if closed else 0]},
                         {"bufferView": 1, "componentType": 5123, "count": len(indices), "type": "SCALAR"}],
           "buffers": [{"byteLength": len(geometry)}],
           "bufferViews": [{"buffer": 0, "byteLength": len(vertices)},
                           {"buffer": 0, "byteOffset": len(vertices), "byteLength": len(faces)}]}
    glb(kind, doc, struct.pack("<I4s", len(geometry), b"BIN\0") + geometry)
    kit = {"kind": kind, "expected_unit_scale": 1, "scale_tolerance": 0.001,
           "sockets": [{"name": "east", "position": [3, 0, 0], "seam_tolerance_m": 0}],
           "allow_non_watertight_foliage": not closed,
           "budget_before": {"vertex_count": 24, "triangle_count": 24, "texture_bytes": 128}}
    spec(kind, kit)
    if kind == "foliage":
        kit["sockets"][0]["position"] = [3.01, 0, 0]
        spec("foliage-seam", kit)

# Keep the real skin and animation buffers while giving the mechanical rig
# its own named hierarchy and target limb.
raw = rig.read_bytes()
end = 20 + struct.unpack_from("<I", raw, 12)[0]
doc = json.loads(raw[20:end])
for index, name in ((0, "turret_base"), (3, "turret_barrel"), (4, "claw_l")):
    doc["nodes"][index]["name"] = name
doc["skins"][0]["name"] = "turret"
glb("turret", doc, raw[end:])

def profile(height, upper, lower):
    return {"height_m": height, "limbs": [{"name": "claw_l", "upper_len_m": upper,
            "lower_len_m": lower, "max_extension_ratio": 0.9}]}

spec("turret-motion", {"source_rig": profile(1.75, 0.5, 0.4), "target_rig": profile(1.4, 0.4, 0.35),
     "contact": {"foot_marker": "claw_l", "stance_frames": [0, 1],
                 "max_slip_m": 0.01, "loop_tolerance_m": 0.02},
     "frames": [{"root_pos": [0, 0, 0], "targets": [{"limb": "claw_l", "hip_pos": [x, 0.5, 0],
                 "end_effector_pos": [x, 0, 0], "foot_contact_pos": [x, 0, 0] if i < 2 else None}]}
                for i, x in enumerate((0, 0.005, 0))]})

def actor(name, root, anchor):
    return {"actor_id": name, "timestamps_ms": list(range(0, 320, 40)),
            "root_positions_m": [[root, 0, 0]] * 8,
            "anchor_positions_m": [[anchor if 2 <= i <= 4 else root, 0, 0] for i in range(8)]}

spec("turret-interaction", {"scope": "shared_clock_kinematic",
     "actors": [actor("turret_base", 0, 0), actor("turret_barrel", 1, 0.02)],
     "interaction": {"actor_a": "turret_base", "actor_b": "turret_barrel",
                     "contact_window_frames": [2, 4], "separation_window_frames": [5, 7],
                     "max_contact_distance_m": 0.03, "min_separation_distance_m": 0.5,
                     "forbidden_interpenetration_m": 0.01}})

digest = hashlib.sha256(static.read_bytes()).hexdigest()
spec("version", {"asset_id": "static", "version_id": "bm06-static", "content_hash": digest,
     "input_id": "bm06", "source": "local", "authorization": "local fixture declaration",
     "authorized": True, "license_id": "CC0-1.0", "usage_scope": "LOCAL_SAMPLE",
     "source_format": "GLB", "contract_version": "1.0.0", "presence": "UNKNOWN"})
(tmp / "object.sha256").write_text(digest, encoding="ascii")
PY

# BM-01 and BM-02: distinguish static and animated character roles.
expect 0 PASS 'true' validate "$STATIC" --role static-mesh
expect 1 FAIL 'true' validate "$STATIC" --role rigged-character
expect 0 PASS 'true' validate "$RIG" --role rigged-character
expect 0 PASS 'true' motion-strike "$RIG" --with-idle --out "$TMP/strike.glb"
expect 0 PASS 'true' rigged-simple-components "$TMP/strike.glb" --out-dir "$TMP/character"
test -s "$TMP/character/motion.json"

# BM-03: real sprite anchors and independently swappable art.
expect 0 PASS 'r.frame_count === 4 && r.frames.every(f => f.anchor[0] === 0 && f.anchor[1] === 0.5)' \
    sprite-inspect "$TMP/sheet.png" --spec "$TMP/sprite.json"
expect 1 FAIL 'v.errors.some(e => e.message === "loop bounds must select an inclusive frame range")' \
    sprite-inspect "$TMP/sheet.png" --spec "$TMP/sprite-no-loop.json"
for kind in ui card tile; do
    expect 0 PASS 'r.visual_quality_status === "NOT_RUN"' \
        card-template-check "$TMP/$kind.json" --illustration "$TMP/oak.png"
    original=$reply
    expect 0 PASS 'r.visual_quality_status === "NOT_RUN"' \
        card-template-check "$TMP/$kind.json" --illustration "$TMP/ash.png"
    printf '%s\n%s\n' "$original" "$reply" | node -e '
const [a, b] = require("fs").readFileSync(0, "utf8").trimEnd().split("\n").map(line => JSON.parse(line).results);
if (!/^[a-f0-9]{64}$/.test(a.template_layout_sha256) ||
    a.template_layout_sha256 !== b.template_layout_sha256 ||
    a.illustration_sha256 === b.illustration_sha256) {
    console.error("illustration swap did not preserve layout and change art");
    process.exit(1);
}'
done
expect 1 FAIL 'v.errors.some(e => e.message === "TextOverflow" || e.code === "TextOverflow")' \
    card-template-check "$TMP/card-overflow.json" --illustration "$TMP/oak.png"

# BM-04 and the additional generic mechanical rig checks.
expect 0 PASS 'r.triangle_count === 12 && r.unclosed_boundary_edges === 0' \
    static-kit-inspect "$TMP/architecture.glb" --spec "$TMP/architecture.json"
expect 0 PASS 'r.unclosed_boundary_edges === 4 && r.double_sided_material_count === 1 &&
    r.alpha_modes.includes("MASK")' static-kit-inspect "$TMP/foliage.glb" --spec "$TMP/foliage.json"
expect 1 FAIL 'r.violations.some(x => x.field === "seam")' \
    static-kit-inspect "$TMP/foliage.glb" --spec "$TMP/foliage-seam.json"
expect 0 PASS 'true' validate "$TMP/turret.glb" --role rigged-character
expect 0 PASS 'r.stages.length === 4 && r.stages.every(s => s.status === "PASS")' \
    motion-retarget "$TMP/turret-motion.json"
expect 0 PASS 'r.visual_engine_status === "NOT_RUN"' motion-interaction "$TMP/turret-interaction.json"

# BM-05: run the presentation suite through the same executable.
presentation=$(ASSETWEFT_BIN="$BIN" "$ROOT/examples/presentation-bundle/verify_presentation_bundle.sh")
printf '%s' "$presentation" | node -e '
const v = JSON.parse(require("fs").readFileSync(0, "utf8"));
if (v.status !== "PASS" || v.checks.combat_clips !== 5 ||
    v.checks.engine_contracts !== 3 || v.checks.audio_tracks !== 2) process.exit(1);
'

# BM-06: keep a real catalog object across the SQLite snapshot and reject tampering.
db=$TMP/library.sqlite
digest=$(cat "$TMP/object.sha256")
expect 0 PASS 'true' library init --db "$db"
expect 0 PASS 'true' catalog commit-object --db "$db" --version "$TMP/version.json" \
    --file "$STATIC" --expected-len "$(wc -c < "$STATIC" | tr -d ' ')"
expect 0 PASS 'r.scope === "backed_up_local_library"' library backup --db "$db" --out "$TMP/backup"
expect 0 PASS 'r.scope === "restored_local_library"' \
    library restore --from "$TMP/backup" --dest "$TMP/restored"
expect 0 PASS 'r.matches.length === 1 && r.matches[0].version_id === "bm06-static"' \
    catalog list --db "$TMP/restored/library.sqlite"
test "$(sqlite3 "$TMP/restored/library.sqlite" 'PRAGMA integrity_check;')" = ok
cmp "$STATIC" "$TMP/restored/objects/$digest"
printf 'corrupt fixture' > "$TMP/backup/objects/$digest"
expect 2 INVALID_INPUT 'r === null && v.errors[0].field_path === "library"' \
    library restore --from "$TMP/backup" --dest "$TMP/refused"
test ! -e "$TMP/refused/manifest.json"

printf '%s\n' '{"status":"PASS","offline_matrix":{"BM-01":"PASS","BM-02":"PASS","BM-03":"PASS","BM-04":"PASS","BM-05":"PASS","BM-06":"PASS"},"mechanical_rig":"PASS","v1_non_goals":["full_game_runtime_ai","commercial_asset_ripping","full_dcc_replacement","mandatory_cloud_account"],"user_task_observations":"NOT_RUN","market_assumptions_report":"SEPARATE_UNVALIDATED_NOT_RUN","live_engine_viewports":"NOT_RUN"}'
