"""Fixed optional Blender worker. No user Python/scripts, network or startup data.
Called only by rig_blender.rs; coordinates reported in glTF meters +Y-up space.
"""
import bpy, json, math, sys, hashlib
from pathlib import Path
from mathutils import Matrix, Vector
out = Path(sys.argv[sys.argv.index('--')+1])
assert bpy.app.version_string == '5.1.2'
assert bpy.app.build_hash.decode() == 'ec6e62d40fa9'
settings = dict(bone_heuristic='BLENDER', guess_original_bind_pose=False,
                disable_bone_shape=True, import_pack_images=False)
def reset():
    bpy.ops.wm.read_factory_settings(use_empty=True)
def load(path):
    bpy.ops.import_scene.gltf(filepath=str(path), **settings)
    meshes=[o for o in bpy.context.scene.objects if o.type=='MESH']
    arms=[o for o in bpy.context.scene.objects if o.type=='ARMATURE']
    assert len(meshes)==len(arms)==1, 'UNSUPPORTED imported object counts'
    obj,arm=meshes[0],arms[0]
    assert set(arm.pose.bones.keys())=={'root','bend'}, 'UNSUPPORTED bone identity mapping'
    assert not any(b.constraints for b in arm.pose.bones), 'UNSUPPORTED constraints'
    assert set(obj.vertex_groups.keys())=={'root','bend'}, 'UNSUPPORTED group identity mapping'
    mods=[m for m in obj.modifiers if m.type=='ARMATURE' and m.object==arm]
    assert len(mods)==1 and len(obj.modifiers)==1, 'UNSUPPORTED driver binding'
    return obj,arm,mods[0]
def coords(obj):
    bpy.context.view_layer.update()
    eo=obj.evaluated_get(bpy.context.evaluated_depsgraph_get()); mesh=eo.to_mesh()
    try:
        # Blender world +Z up -> glTF world +Y up.
        return [[v.x,v.z,-v.y] for v in (eo.matrix_world @ x.co for x in mesh.vertices)]
    finally: eo.to_mesh_clear()
reset();obj,arm,mod=load(out/'input.glb')
pose=arm.pose.bones['bend']; rest=pose.matrix.copy(); pivot=rest.translation
samples={}
for key,angle,enabled in [('rest',0,True),('positive',math.pi/2,True),('negative',-math.pi/2,True),('disabled_positive',math.pi/2,False)]:
    # Fixture armature is identity in world. glTF +Z is Blender -Y.
    pose.matrix=Matrix.Translation(pivot) @ Matrix.Rotation(angle,4,'Y').transposed() @ Matrix.Translation(-pivot) @ rest
    mod.show_viewport=enabled; samples[key]=coords(obj)
mod.show_viewport=True
# Export an actual keyed bend clip; exporter retains rest binding.
bpy.context.scene.frame_start=1; bpy.context.scene.frame_end=2
for frame,angle in [(1,0),(2,math.pi/2)]:
    bpy.context.scene.frame_set(frame)
    pose.matrix=Matrix.Translation(pivot) @ Matrix.Rotation(-angle,4,'Y') @ Matrix.Translation(-pivot) @ rest
    pose.keyframe_insert(data_path='rotation_quaternion',frame=frame)
bpy.context.scene.frame_set(2)
export_settings=dict(export_format='GLB',export_yup=True,export_skins=True,
    export_animations=True,export_force_sampling=True,export_frame_range=True,
    export_current_frame=False,export_rest_position_armature=True,export_apply=False)
bpy.ops.export_scene.gltf(filepath=str(out/'roundtrip.glb'),**export_settings)
reset();obj,arm,mod=load(out/'roundtrip.glb')
bpy.context.scene.frame_set(1); samples['reimport_rest']=coords(obj)
bpy.context.scene.frame_set(2); samples['reimport_positive']=coords(obj)
r=dict(version=bpy.app.version_string,build_hash=bpy.app.build_hash.decode(),
    source='TEST_FIXTURE',space='glTF meters right-handed +Y up +Z forward',
    identity_mapping={'root':'root','bend':'bend'},import_settings=settings,
    export_settings=export_settings,samples=samples,
    input_sha256=hashlib.sha256((out/'input.glb').read_bytes()).hexdigest(),
    output_sha256=hashlib.sha256((out/'roundtrip.glb').read_bytes()).hexdigest())
(out/'result.json').write_text(json.dumps(r,indent=2)+'\n')
