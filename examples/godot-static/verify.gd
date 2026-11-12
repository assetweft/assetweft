# Responsibility: check analytic static fixture through Godot's importer.
# Dependencies: Godot 4.7.2 GLTFDocument; caller: headless CLI.
# Invariants: fixed local fixture paths, no scripts from assets, no network;
# expected geometry is analytic, never learned from imported output.
extends SceneTree

func fail(kind: String, detail: String) -> void:
	print(JSON.stringify({"status":"FAIL", "kind":kind, "detail":detail}))
	quit(2)

func _initialize() -> void:
	call_deferred("verify")

func verify() -> void:
	var args := OS.get_cmdline_user_args()
	var path := "res://triangle.glb"
	if args.has("--malformed"):
		path = "res://malformed.glb"
	var doc := GLTFDocument.new()
	var state := GLTFState.new()
	var code := doc.append_from_file(path, state)
	if code != OK:
		fail("IMPORT_FAILED", str(code))
		return
	var scene := doc.generate_scene(state)
	if scene == null:
		fail("SCENE_MISSING", path)
		return
	root.add_child(scene)
	var nodes := scene.find_children("*", "MeshInstance3D", true, false)
	if scene is MeshInstance3D:
		nodes.append(scene)
	if nodes.size() != 1:
		fail("MESH_COUNT", str(nodes.size()))
		return
	var instance: MeshInstance3D = nodes[0]
	var mesh := instance.mesh
	if mesh == null or mesh.get_surface_count() != 1:
		fail("SURFACE_COUNT", "expected one surface")
		return
	var arrays := mesh.surface_get_arrays(0)
	var vertices: PackedVector3Array = arrays[Mesh.ARRAY_VERTEX]
	var indices: PackedInt32Array = arrays[Mesh.ARRAY_INDEX]
	if vertices.size() != 3 or indices.size() != 3:
		fail("GEOMETRY_COUNT", str([vertices.size(), indices.size()]))
		return
	var bounds := AABB(instance.global_transform * vertices[0], Vector3.ZERO)
	for v in vertices:
		bounds = bounds.expand(instance.global_transform * v)
	var expected_size := Vector3(2, 3, 0)
	if args.has("--wrong-bounds"):
		expected_size = Vector3(99, 3, 0)
	if not bounds.position.is_equal_approx(Vector3(1, 2, 3)) or not bounds.size.is_equal_approx(expected_size):
		fail("BOUNDS_MISMATCH", str(bounds))
		return
	var material := mesh.surface_get_material(0) as StandardMaterial3D
	if material == null or not material.albedo_color.is_equal_approx(Color(0, 1, 0, 1)) or not is_equal_approx(material.metallic, 0.0) or not is_equal_approx(material.roughness, 1.0):
		fail("MATERIAL_MISMATCH", str(material))
		return
	for slot in range(BaseMaterial3D.TEXTURE_MAX):
		if material.get_texture(slot) != null:
			fail("UNEXPECTED_TEXTURE", str(slot))
			return
	print(JSON.stringify({"status":"PASS", "engine":Engine.get_version_info().string, "mesh_instances":nodes.size(), "vertices":vertices.size(), "indices":indices.size(), "bounds_position":str(bounds.position), "bounds_size":str(bounds.size), "material":material.resource_name, "albedo":str(material.albedo_color), "textures":"absent: analytic fixture declares none", "external_dependencies":0, "source_sha256":FileAccess.get_sha256(path), "render":"NOT_RUN"}))
	scene.free()
	quit(0)
