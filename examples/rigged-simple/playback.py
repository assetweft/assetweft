"""Optional Blender verification of the delivered GLB and its preserved original clip.
Run with --factory-startup --disable-autoexec --python-exit-code 1 --python playback.py
-- SOURCE.glb OUTPUT.glb [--with-idle]. Measures evaluated world-space mesh and bones in metres.
"""
import hashlib
import json
import math
import struct
import sys
from pathlib import Path

import bpy
from mathutils import Matrix

IDLE = "AssetWeft_Idle"
CLIP = "AssetWeft_InPlaceStrike"
FRAMES = [0, 3, 6, 9, 12, 18, 24]
EXPECTED_DEGREES = [0, 0, 0, 45, 90, 45, 0]


def read_glb(path):
    data = path.read_bytes()
    end = 20 + struct.unpack_from("<I", data, 12)[0]
    return hashlib.sha256(data).hexdigest(), json.loads(data[20:end]), data[end + 8:]


def check_preservation(source, output, with_idle):
    src_hash, src, src_bin = read_glb(source)
    out_hash, out, out_bin = read_glb(output)
    assert src_hash == "3a79dabb67bb0cd598a18d08b954d9d357c27c30672f82ef5d3f4e7fe6ca3401"
    assert out_bin.startswith(src_bin)
    added = out["animations"][len(src["animations"]):]
    assert [a["name"] for a in added] == ([CLIP, IDLE] if with_idle else [CLIP])
    for clip in added:
        assert clip["channels"] == [{"sampler": 0, "target": {"node": 4, "path": "rotation"}}]
    for key in ("animations", "accessors", "bufferViews"):
        out[key] = out[key][:len(src[key])]
    assert out["buffers"][0]["byteLength"] == len(out_bin)
    out["buffers"][0]["byteLength"] = len(src_bin)
    assert "2017 Cesium" in out["asset"].pop("copyright")
    assert out == src, "source scene, skin, mesh or original clip changed"
    return src_hash, out_hash


def pose(arm, mesh):
    graph = bpy.context.evaluated_depsgraph_get()
    rig = arm.evaluated_get(graph)
    root = rig.matrix_world @ rig.pose.bones["Bone"].matrix
    child = rig.matrix_world @ rig.pose.bones["Bone.001"].matrix
    obj = mesh.evaluated_get(graph)
    verts = [obj.matrix_world @ v.co for v in obj.data.vertices]
    assert len(verts) == 160
    return root, child, verts


def sample(action, arm, mesh, repeat=False):
    anim = arm.animation_data
    anim.use_nla = False
    anim.action = None
    for bone in arm.pose.bones:
        bone.matrix_basis = Matrix.Identity(4)
    bpy.context.view_layer.update()
    rest_root, rest_child, rest_verts = pose(arm, mesh)
    pivot = rest_child.translation
    # Track a fully distal vertex far from the rotation axis.
    group = mesh.vertex_groups["Bone.001"].index
    rigid = [v.index for v in mesh.data.vertices
             if any(g.group == group and g.weight > 0.99999 for g in v.groups)]
    tip = max(rigid, key=lambda i: math.hypot(rest_verts[i].y - pivot.y, rest_verts[i].z - pivot.z))
    if repeat:
        anim.use_nla = True
        for track in anim.nla_tracks:
            track.mute = True
        strip = anim.nla_tracks.new().strips.new(IDLE, 0, action)
        strip.action_slot = action.slots[0]
        strip.repeat = 2
        assert (strip.frame_start, strip.frame_end) == (0, 48)
    else:
        anim.action = action
        anim.action_slot = action.slots[0]
    scene = bpy.context.scene
    samples = []
    phases = {}
    for frame in (range(49) if repeat else FRAMES):
        scene.frame_set(frame)
        root, child, verts = pose(arm, mesh)
        delta = child.to_quaternion() @ rest_child.to_quaternion().inverted()
        # The reference glTF local +Y axis maps to Blender world +X.
        before, after = rest_verts[tip] - pivot, verts[tip] - pivot
        skin_deg = math.degrees(math.atan2(before.y*after.z-before.z*after.y,
                                          before.y*after.y+before.z*after.z))
        phase_error = 0.0
        if repeat:
            reference = phases.setdefault(frame % 24, verts)
            phase_error = max((v-r).length for v, r in zip(verts, reference))
        samples.append(dict(frame=frame, angle_deg=math.degrees(2*math.atan2(delta.x, delta.w)),
                            off_axis=math.hypot(delta.y, delta.z), tip_index=tip,
                            skin_deg=skin_deg, phase_error_m=phase_error,
                            root_delta=max(abs(root[i][j] - rest_root[i][j])
                                           for i in range(4) for j in range(4)),
                            mesh_displacement_m=max((v-r).length for v, r in zip(verts, rest_verts))))
    return samples


def verify_curve(samples, idle=False):
    for i, s in enumerate(samples):
        if idle:
            phase = s["frame"] % 24
            # Triangle wave: rest → +8° → -8° → rest in one second.
            steps = phase if phase <= 6 else 12 - phase
            if phase > 18:
                steps = phase - 24
            angle = steps * 8 / 6
        else:
            angle = EXPECTED_DEGREES[i]
        assert s["off_axis"] < 1e-5, ("unexpected axis", s)
        assert abs(s["skin_deg"] - angle) < 0.1, ("signed skin angle", s, angle)
        assert s["phase_error_m"] < 1e-5, ("loop mismatch", s)
        assert s["root_delta"] < 1e-5, ("root moved", s)
        assert abs(s["angle_deg"] - angle) < 0.1, ("child angle", s)
        # Rigid distal vertices on this cylinder sweep a chord ~2*r*sin(theta/2).
        if idle:
            expected = 2 * 4.59 * math.sin(math.radians(abs(angle))/2)
            assert abs(s["mesh_displacement_m"] - expected) < 0.005, ("idle deformation", s)
            continue
        low, high = {0: (-1e-6, 1e-5), 45: (3.4, 3.7), 90: (6.3, 6.7)}[angle]
        assert low < s["mesh_displacement_m"] < high, ("evaluated mesh", s)


def main(source, output, with_idle):
    src_hash, out_hash = check_preservation(source, output, with_idle)
    bpy.ops.wm.read_factory_settings(use_empty=True)
    scene = bpy.context.scene
    scene.render.fps = 24
    bpy.ops.import_scene.gltf(filepath=str(output), bone_heuristic="BLENDER",
                              guess_original_bind_pose=False, disable_bone_shape=True)
    arm, mesh = scene.objects["Armature"], scene.objects["Cylinder"]
    original, = [action for action in bpy.data.actions if action.name not in (CLIP, IDLE)]
    samples = sample(bpy.data.actions[CLIP], arm, mesh)
    verify_curve(samples)
    control = sample(original, arm, mesh)
    try:
        verify_curve(control)
    except AssertionError as exc:
        reason = str(exc)
    else:
        raise AssertionError("original clip unexpectedly passed strike playback")
    idle = []
    if with_idle:
        idle = sample(bpy.data.actions[IDLE], arm, mesh, repeat=True)
        verify_curve(idle, idle=True)
    print(json.dumps(dict(idle=idle, source_sha256=src_hash, output_sha256=out_hash,
                          blender=bpy.app.version_string, build=bpy.app.build_hash.decode(),
                          fps=scene.render.fps, clip=CLIP, samples=samples,
                          original=original.name, original_samples=control, rejected_by=reason)))


args = sys.argv[sys.argv.index("--") + 1:]
assert args[2:] in ([], ["--with-idle"])
main(Path(args[0]).resolve(), Path(args[1]).resolve(), bool(args[2:]))
