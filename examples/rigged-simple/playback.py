"""Optional Blender verification of the delivered GLB and its preserved original clip.
Run with --factory-startup --disable-autoexec --python-exit-code 1 --python playback.py
-- SOURCE.glb OUTPUT.glb. Measures evaluated world-space mesh and bones in metres.
"""
import hashlib
import json
import math
import struct
import sys
from pathlib import Path

import bpy
from mathutils import Matrix

CLIP = "AssetWeft_InPlaceStrike"
FRAMES = [0, 3, 6, 9, 12, 18, 24]
EXPECTED_DEGREES = [0, 0, 0, 45, 90, 45, 0]


def read_glb(path):
    data = path.read_bytes()
    end = 20 + struct.unpack_from("<I", data, 12)[0]
    return hashlib.sha256(data).hexdigest(), json.loads(data[20:end]), data[end + 8:]


def check_preservation(source, output):
    src_hash, src, src_bin = read_glb(source)
    out_hash, out, out_bin = read_glb(output)
    assert src_hash == "3a79dabb67bb0cd598a18d08b954d9d357c27c30672f82ef5d3f4e7fe6ca3401"
    assert out_bin.startswith(src_bin)
    assert len(out["animations"]) == len(src["animations"]) + 1
    for key in ("animations", "accessors", "bufferViews"):
        out[key] = out[key][:len(src[key])]
    assert out["buffers"][0]["byteLength"] == len(out_bin)
    out["buffers"][0]["byteLength"] = len(src_bin)
    assert "2017 Cesium" in out["asset"].pop("copyright")
    assert out == src, "source scene, skin, mesh or original clip changed"
    return src_hash, out_hash


def sample(action, arm, mesh):
    anim = arm.animation_data
    anim.use_nla = False
    anim.action = None
    for bone in arm.pose.bones:
        bone.matrix_basis = Matrix.Identity(4)
    anim.action = action
    anim.action_slot = action.slots[0]
    scene = bpy.context.scene
    samples = []
    for frame in FRAMES:
        scene.frame_set(frame)
        graph = bpy.context.evaluated_depsgraph_get()
        rig = arm.evaluated_get(graph)
        root = rig.matrix_world @ rig.pose.bones["Bone"].matrix
        child = rig.matrix_world @ rig.pose.bones["Bone.001"].matrix
        obj = mesh.evaluated_get(graph)
        verts = [obj.matrix_world @ v.co for v in obj.data.vertices]
        assert len(verts) == 160
        if not samples:
            rest_root, rest_child, rest_verts = root, child.to_quaternion(), verts
        delta = rest_child.rotation_difference(child.to_quaternion())
        samples.append(dict(frame=frame, angle_deg=math.degrees(delta.angle),
                            root_delta=max(abs(root[i][j] - rest_root[i][j])
                                           for i in range(4) for j in range(4)),
                            mesh_displacement_m=max((v-r).length for v, r in zip(verts, rest_verts))))
    return samples


def verify_curve(samples):
    for s, angle in zip(samples, EXPECTED_DEGREES, strict=True):
        assert s["root_delta"] < 1e-5, ("root moved", s)
        assert abs(s["angle_deg"] - angle) < 0.1, ("child angle", s)
        # Rigid distal vertices on this cylinder sweep a chord ~2*r*sin(theta/2).
        low, high = {0: (-1e-6, 1e-5), 45: (3.4, 3.7), 90: (6.3, 6.7)}[angle]
        assert low < s["mesh_displacement_m"] < high, ("evaluated mesh", s)


def main(source, output):
    src_hash, out_hash = check_preservation(source, output)
    bpy.ops.wm.read_factory_settings(use_empty=True)
    scene = bpy.context.scene
    scene.render.fps = 24
    bpy.ops.import_scene.gltf(filepath=str(output), bone_heuristic="BLENDER",
                              guess_original_bind_pose=False, disable_bone_shape=True)
    arm, mesh = scene.objects["Armature"], scene.objects["Cylinder"]
    original, = [action for action in bpy.data.actions if action.name != CLIP]
    samples = sample(bpy.data.actions[CLIP], arm, mesh)
    verify_curve(samples)
    control = sample(original, arm, mesh)
    try:
        verify_curve(control)
    except AssertionError as exc:
        reason = str(exc)
    else:
        raise AssertionError("original clip unexpectedly passed strike playback")
    print(json.dumps(dict(source_sha256=src_hash, output_sha256=out_hash,
                          blender=bpy.app.version_string, build=bpy.app.build_hash.decode(),
                          fps=scene.render.fps, clip=CLIP, samples=samples,
                          original=original.name, original_samples=control, rejected_by=reason)))


main(*(Path(p).resolve() for p in sys.argv[sys.argv.index("--") + 1:]))
