"""Measure the pinned Cesium Rigged Simple base pose using Blender's evaluated mesh.
Called by Assurance only for the original licensed file. Never play or export its clip.
"""
import hashlib
import json
import math
import sys
from pathlib import Path

import bpy
from mathutils import Matrix


def world_xyz(point):
    # Blender world to glTF world, metres.
    return [point.x, point.z, -point.y]


def measure(out):
    path = out / "input.glb"
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    assert digest == "3a79dabb67bb0cd598a18d08b954d9d357c27c30672f82ef5d3f4e7fe6ca3401"
    assert (bpy.app.version_string, bpy.app.build_hash.decode()) == ("5.1.2", "ec6e62d40fa9")
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=str(path), bone_heuristic="BLENDER",
                              guess_original_bind_pose=False, disable_bone_shape=True,
                              import_pack_images=False)
    mesh = bpy.context.scene.objects["Cylinder"]
    arm = bpy.context.scene.objects["Armature"]
    assert mesh.type == "MESH" and arm.type == "ARMATURE"
    assert set(mesh.vertex_groups.keys()) == set(arm.pose.bones.keys()) == {"Bone", "Bone.001"}
    assert len(mesh.modifiers) == 1
    modifier = mesh.modifiers[0]
    assert modifier.type == "ARMATURE" and modifier.object == arm
    arm.animation_data_clear()
    for bone in arm.pose.bones:
        assert not bone.constraints
        bone.matrix_basis = Matrix.Identity(4)
    bpy.context.view_layer.update()
    bone = arm.pose.bones["Bone.001"]
    assert bone.parent.name == "Bone"
    original = bone.matrix.copy()
    pivot = arm.matrix_world @ original.translation
    # A world +Z bend in glTF corresponds to Blender world -Y. The armature itself is rotated.
    turn = Matrix.Translation(pivot) @ Matrix.Rotation(-math.pi / 2, 4, 'Y') @ Matrix.Translation(-pivot)
    target = arm.matrix_world.inverted() @ turn @ arm.matrix_world @ original
    samples, joint = {}, {}
    for key, pose, enabled in [("rest", original, True), ("positive", target, True),
                               ("disabled_positive", target, False)]:
        bone.matrix = pose
        modifier.show_viewport = enabled
        bpy.context.view_layer.update()
        eo = mesh.evaluated_get(bpy.context.evaluated_depsgraph_get())
        evaluated = eo.to_mesh()
        try:
            samples[key] = [world_xyz(eo.matrix_world @ v.co) for v in evaluated.vertices]
        finally:
            eo.to_mesh_clear()
        joint[key] = dict(head=world_xyz(arm.matrix_world @ bone.head),
                          tail=world_xyz(arm.matrix_world @ bone.tail))
    return dict(input_sha256=digest, version=bpy.app.version_string, build_hash=bpy.app.build_hash.decode(),
                source="Cesium Rigged Simple (2017), CC BY 4.0", space="glTF world metres +Y up",
                mesh_node=2, mesh=0, skin=0, joint_node=4, joint_name=bone.name, angle_degrees=90,
                axis="world +Z", joint=joint, samples=samples)


out = Path(sys.argv[sys.argv.index('--') + 1])
(out / "result.json").write_text(json.dumps(measure(out), allow_nan=False) + "\n")
