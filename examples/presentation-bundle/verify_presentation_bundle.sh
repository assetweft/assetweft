#!/bin/sh
set -eu

[ "$#" -eq 0 ] || { printf 'usage: %s\n' "$0" >&2; exit 2; }
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
BIN=${ASSETWEFT_BIN:-"$ROOT/target/debug/assetweft"}
[ -x "$BIN" ] || { printf 'ASSETWEFT_BIN is not executable: %s\n' "$BIN" >&2; exit 2; }
for tool in node python3; do
    command -v "$tool" >/dev/null || { printf '%s is required\n' "$tool" >&2; exit 2; }
done

TMP=$(mktemp -d "${TMPDIR:-/tmp}/assetweft-aw035b.XXXXXX")
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

RIG=$ROOT/examples/rigged-simple/RiggedSimple.glb
python3 - "$TMP" "$RIG" <<'PY'
import copy
import hashlib
import json
from pathlib import Path
import struct
import sys
import wave
import zlib

tmp, rig = map(Path, sys.argv[1:])

def spec(name, value):
    (tmp / (name + ".json")).write_text(json.dumps(value), encoding="utf-8")

def glb(name, doc, chunks):
    data = json.dumps(doc).encode()
    data += b" " * (-len(data) % 4)
    (tmp / (name + ".glb")).write_bytes(
        struct.pack("<4sIII4s", b"glTF", 2, 20 + len(data) + len(chunks), len(data), b"JSON")
        + data + chunks
    )

raw = rig.read_bytes()
end = 20 + struct.unpack_from("<I", raw, 12)[0]
doc, chunks = json.loads(raw[20:end]), raw[end:]
attributes = doc["meshes"][0]["primitives"][0]["attributes"]
mesh = {"primitives": [{"attributes": {"POSITION": attributes["POSITION"]}}]}
doc["meshes"].append(mesh)
doc["nodes"][2]["name"] = "tunic"
body = len(doc["nodes"])
doc["nodes"].extend([{"name": "body", "mesh": 1}, {"name": "buckle", "mesh": 1},
                     {"name": "socket", "children": [body + 3]},
                     {"name": "part", "mesh": 1, "translation": [0.0005, 0, 0]}])
doc["scenes"][0]["nodes"].extend([body, body + 1, body + 2])
glb("bundle", doc, chunks)
doc["nodes"][body + 3]["translation"][0] = 0.0006
glb("over-drift", doc, chunks)
spec("face", {"face_mode": "not-applicable", "expected_expressions": [],
              "rigid_attachments": [{"socket_node": "socket", "attachment_node": "part",
                                     "expected_offset_m": [0, 0, 0],
                                     "max_drift_m": 0.0005, "lock_mode": "BYTE"}]})

# Read both output meshes back. Bind the attachment primitive, accessor and
# referenced buffer bytes, excluding the intentionally varied node transform.
def attachment_sha(path):
    raw = path.read_bytes()
    end = 20 + struct.unpack_from("<I", raw, 12)[0]
    doc = json.loads(raw[20:end])
    node = next(node for node in doc["nodes"] if node.get("name") == "part")
    mesh = doc["meshes"][node["mesh"]]
    accessor = doc["accessors"][mesh["primitives"][0]["attributes"]["POSITION"]]
    view = doc["bufferViews"][accessor["bufferView"]]
    start = end + 8 + view.get("byteOffset", 0)
    metadata = json.dumps([mesh, accessor, view], sort_keys=True).encode()
    return hashlib.sha256(metadata + raw[start:start + view["byteLength"]]).hexdigest()

before, after = (attachment_sha(tmp / name) for name in ("bundle.glb", "over-drift.glb"))
if before != after:
    raise SystemExit("locked attachment mesh changed")
(tmp / "mesh.sha256").write_text(before, encoding="ascii")

def rig_spec(height, upper, lower):
    return {"height_m": height, "limbs": [{"name": "foot", "upper_len_m": upper,
            "lower_len_m": lower, "max_extension_ratio": 0.9}]}

def frame(x, height, contact):
    return {"root_pos": [0, 0, 0], "targets": [{"limb": "foot", "hip_pos": [x, 0.5, 0],
            "end_effector_pos": [x, height, 0], "foot_contact_pos": contact}]}

for name, swing in (("idle", 0), ("attack", 0.03), ("skill", 0.08),
                    ("hit_react", 0.04), ("death", 0.12)):
    spec(name, {"source_rig": rig_spec(1.75, 0.5, 0.4), "target_rig": rig_spec(1.4, 0.4, 0.35),
                "contact": {"foot_marker": "foot", "stance_frames": [0, 1],
                            "max_slip_m": 0.01, "loop_tolerance_m": 0.02},
                "frames": [frame(0, 0, [0, 0, 0]), frame(0.005, 0, [0.005, 0, 0]),
                           frame(0.01, swing, None), frame(0, 0, None)]})

def actor(name, root, anchor):
    return {"actor_id": name, "timestamps_ms": list(range(0, 320, 40)),
            "root_positions_m": [[root, 0, 0]] * 8,
            "anchor_positions_m": [[anchor if 2 <= i <= 4 else root, 0, 0] for i in range(8)]}

spec("interaction", {"scope": "shared_clock_kinematic",
     "actors": [actor("attacker", 0, 0), actor("defender", 1, 0.02)],
     "interaction": {"actor_a": "attacker", "actor_b": "defender", "contact_window_frames": [2, 4],
                     "separation_window_frames": [5, 7], "max_contact_distance_m": 0.03,
                     "min_separation_distance_m": 0.5, "forbidden_interpenetration_m": 0.01}})

spec("cloth", {"garments": [
    {"node_name": node, "binding_kind": binding, "body_mesh_node": "body",
     "max_weight_sum_error": 0.0001, "max_stretch_ratio": 0.1, "max_penetration_m": 0.05,
     "require_supported_structure": True} for node, binding in (("tunic", "skinned"), ("buckle", "rigid"))],
    "sampled_motion_poses": [{"frame": i, "body_vertices": [[0, 0, 0], [1, 1, 1]],
      "garment_vertices": [[0, 0, 1.5], [1, 0, 1.5]], "rest_garment_edges": [[0, 1]],
      "reference_edge_lengths_m": [1]} for i in (1, 2)]})

def png_chunk(kind, data):
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

(tmp / "atlas.png").write_bytes(b"\x89PNG\r\n\x1a\n"
    + png_chunk(b"IHDR", struct.pack(">IIBBBBB", 2, 1, 8, 6, 0, 0, 0))
    + png_chunk(b"IDAT", zlib.compress(bytes([0, 1, 2, 3, 0, 1, 2, 3, 255])))
    + png_chunk(b"IEND", b""))
spec("vfx", {"vfx_mode": "flipbook", "authoring_state": "native_editable",
     "requires_native_editable": True, "timing": {"fps": 20, "frame_count": 2, "duration_ms": 100,
     "loop_cycle": True, "blend_mode": "alpha"}, "parameters": [{"name": "intensity", "value": 0.5,
     "min": 0, "max": 1}], "flipbook": {"frame_width": 1, "frame_height": 1, "columns": 2,
     "rows": 1, "require_transparency": True}})

action = {"clip_duration_ms": 300,
          "phases": {"charge_ms": [0, 100], "release_ms": [100, 200], "recovery_ms": [200, 300]},
          "bindings": [
              {"id": "glow", "kind": "vfx", "phase": "charge", "trigger_ms": 0, "duration_ms": 100,
               "cancel_cleanup_policy": "stop_and_clear", "event_source": "fixed_timeline",
               "claims_gameplay_hit": False},
              {"id": "impact", "kind": "sfx", "phase": "release", "trigger_ms": 150, "duration_ms": 30,
               "cancel_cleanup_policy": "stop_and_clear", "event_source": "authoritative_hit",
               "claims_gameplay_hit": True},
              {"id": "tail", "kind": "sfx", "phase": "recovery", "trigger_ms": 200, "duration_ms": 100,
               "cancel_cleanup_policy": "fade_out", "event_source": "fixed_timeline",
               "claims_gameplay_hit": False}]}
spec("action", action)
action["bindings"][0]["cancel_cleanup_policy"] = "persist_leak"
spec("action-leak", action)

for name, samples in (("voice", (0, 4096, -4096, 0)), ("impact", (0, 8192, -8192, 0))):
    with wave.open(str(tmp / (name + ".wav")), "wb") as wav:
        wav.setparams((1, 2, 4, 0, "NONE", "not compressed"))
        wav.writeframes(struct.pack("<4h", *samples))
track = {"track_id": "voice", "model_id": "tts-fixture", "wav_path": "voice.wav",
         "expected_channels": 1, "expected_sample_rate_hz": 4,
         "min_duration_ms": 1000, "max_duration_ms": 1000,
         "licensing": {"code_license": "GPL-3.0", "weights_license": "MIT",
                       "dataset_license": "CC0-1.0", "service_tos_ref": "offline",
                       "linkage": "external_subprocess", "dependency_name": "piper1-gpl"}}
voice = copy.deepcopy(track)
voice.update(text_version="v1", script_text="Ready!", voice_id="voice-fixture",
             external_runtime_status="NOT_RUN")
sfx = dict(track, track_id="impact", model_id="sfx-fixture", wav_path="impact.wav",
           category="impact", loop_region={"start_sample": 0, "end_sample": 4,
           "max_step_delta": 0.125, "max_zero_crossing_offset": 0})
audio = {"voice": voice, "sfx": [sfx], "bgm": None}
spec("audio", audio)
voice["licensing"]["linkage"] = "in_process"
spec("audio-gpl", audio)

source = [0, 1, 0, 0, 0, 0, 1, 0, 1, 0, 0, 0, 4, 5, 6, 1]
for engine, adapter, units, position, ibm in (
    ("unity", "unity_csharp", "m", [-1, 2, 3], [0, -1, 0, 0, 0, 0, 1, 0, -1, 0, 0, 0, -4, 5, 6, 1]),
    ("unreal", "unreal_python", "cm", [100, 300, 200], [0, 0, 1, 0, 1, 0, 0, 0, 0, 1, 0, 0, 400, 600, 500, 1]),
    ("godot", "godot_gdscript", "m", [1, 2, 3], source),
):
    left_handed = engine != "godot"
    packed = ({"map_role": "metallic_smoothness", "srgb": False, "channel_order": "R=metallic,A=smoothness",
               "smoothness_inverted_from_roughness": True} if engine == "unity" else
              {"map_role": "orm", "srgb": False, "channel_order": "RGB=ORM"})
    check = {"target_engine": engine, "asset_category": "rigged_action", "engine_version": "1.2.3",
             "plugin_version": "0.1.0", "adapter_kind": adapter, "adapter_entrypoint": "Import",
             "scale_units": units, "dependencies": [],
             "transform_sample": {"source_position_m": [1, 2, 3], "target_position": position,
                "source_triangle": [2, 3, 4], "target_triangle": [2, 4, 3] if left_handed else [2, 3, 4],
                "source_tangent_w": 1, "target_tangent_w": -1 if left_handed else 1,
                "source_ibm_col_major": source, "target_ibm_col_major": ibm, "max_tolerance": 0},
             "pbr_textures": [packed, {"map_role": "normal", "srgb": False}]}
    spec(engine, check)
    if engine == "unity":
        check["pbr_textures"][1]["srgb"] = True
        spec("engine-srgb", check)
PY

for clip in idle attack skill hit_react death; do
    expect 0 PASS 'r.stages.length === 4 && r.stages.every(s => s.status === "PASS")' \
        motion-retarget "$TMP/$clip.json"
done
expect 0 PASS 'r.visual_engine_status === "NOT_RUN"' motion-interaction "$TMP/interaction.json"
expect 0 PASS 'r.attachment_results[0].world_drift_m === 0.0005 && r.attachment_results[0].lock_mode === "BYTE"' \
    face-attachment-inspect "$TMP/bundle.glb" --spec "$TMP/face.json"
expect 1 FAIL 'r.attachment_results[0].world_drift_m === 0.0006 &&
    r.attachment_results[0].violation === "attachment exceeds maximum drift"' \
    face-attachment-inspect "$TMP/over-drift.glb" --spec "$TMP/face.json"
expect 0 PASS 'r.garments.length === 2 && r.visual_engine_status === "NOT_RUN"' \
    cloth-inspect "$TMP/bundle.glb" --spec "$TMP/cloth.json"
expect 0 PASS 'r.flipbook_facts.transparent_pixel_count === 1 && r.visual_engine_status === "NOT_RUN"' \
    vfx-inspect "$TMP/vfx.json" --flipbook "$TMP/atlas.png"
expect 0 PASS 'r.engine_playback_status === "NOT_RUN"' action-presentation-inspect "$TMP/action.json"
expect 1 FAIL 'v.errors.some(e => e.code === "UncleanCancelCleanup")' \
    action-presentation-inspect "$TMP/action-leak.json"
expect 0 PASS 'r.bgm_status === "NOT_RUN" && !r.bgm_generated && r.provenance_entries.length === 2' \
    audio-connector-inspect "$TMP/audio.json"
expect 1 FAIL 'r.violations.includes("CopyleftInProcessViolation")' audio-connector-inspect "$TMP/audio-gpl.json"
for engine in unity unreal godot; do
    expect 0 PASS 'r.engine_editor_runtime_status === "NOT_RUN" && !r.headless_import_is_render_pass' \
        engine-contract-check "$TMP/$engine.json"
done
expect 1 FAIL 'v.errors.some(e => e.code === "DataTextureSrgbViolation")' \
    engine-contract-check "$TMP/engine-srgb.json"

printf '{"status":"PASS","checks":{"combat_clips":5,"interactions":1,"rigid_attachments":1,"garments":2,"vfx":1,"action_presentations":1,"audio_tracks":2,"engine_contracts":3,"negative_controls":4},"q6":{"max_drift_m":0.0005,"pass_world_drift_m":0.0005,"rejected_world_drift_m":0.0006,"locked_mesh_sha256":"%s"},"not_run":{"live_unity_editor_render":"NOT_RUN","live_unreal_editor_render":"NOT_RUN","live_godot_editor_render":"NOT_RUN","independent_ta_review_status":"NOT_RUN","optional_bgm_generation":"NOT_RUN","asr_tts_playback":"NOT_RUN"}}\n' "$(cat "$TMP/mesh.sha256")"
