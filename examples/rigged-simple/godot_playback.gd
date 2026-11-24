# Rigged Simple playback probe. Run on the licensed --with-idle derivative in a scratch
# Godot 4.7.2 project. Only the renderer path can return an engine-baked mesh.
extends SceneTree

const STRIKE = "AssetWeft_InPlaceStrike"
const IDLE = "AssetWeft_Idle"
const TIMES = [0.0, 0.25, 0.375, 0.5, 0.75, 1.0]
const ANGLES = {STRIKE: [0, 0, 45, 90, 45, 0], IDLE: [0, 8, 4, 0, -8, 0]}
var report = {"samples": [], "engine_bake": "NOT_RUN"}
var instance: MeshInstance3D
var skeleton: Skeleton3D

func _initialize() -> void:
	call_deferred("probe")

func finish(status: String, code: int) -> void:
	report["status"] = status
	print(JSON.stringify(report))
	quit(code)

# Independent linear blend reconstruction from the imported surface, Skin and live poses.
func reconstruct(arrays: Array) -> PackedVector3Array:
	var transforms: Array[Transform3D] = []
	for slot in instance.skin.get_bind_count():
		var bone := instance.skin.get_bind_bone(slot)
		var name := instance.skin.get_bind_name(slot)
		if name != &"":
			bone = skeleton.find_bone(name)
		if bone < 0 or bone >= skeleton.get_bone_count():
			return []
		transforms.append(skeleton.get_bone_global_pose(bone) * instance.skin.get_bind_pose(slot))
	var points := PackedVector3Array()
	for i in arrays[Mesh.ARRAY_VERTEX].size():
		var point := Vector3.ZERO
		for j in range(i * 4, i * 4 + 4):
			var slot: int = arrays[Mesh.ARRAY_BONES][j]
			if slot < 0 or slot >= transforms.size():
				return []
			point += (transforms[slot] * arrays[Mesh.ARRAY_VERTEX][i]) * arrays[Mesh.ARRAY_WEIGHTS][j]
		points.append(point)
	return points

func max_distance(a: PackedVector3Array, b: PackedVector3Array) -> float:
	var largest := 0.0
	for i in a.size():
		if not a[i].is_finite() or not b[i].is_finite():
			return INF
		largest = maxf(largest, a[i].distance_to(b[i]))
	return largest

func probe() -> void:
	var args := OS.get_cmdline_user_args()
	if args.size() != 1:
		finish("EXPECTED_GLB_PATH", 2)
		return
	report["sha256"] = FileAccess.get_sha256(args[0])
	report["godot"] = Engine.get_version_info().string
	report["display"] = DisplayServer.get_name()
	report["adapter"] = RenderingServer.get_video_adapter_name()
	var doc := GLTFDocument.new()
	var state := GLTFState.new()
	var error := doc.append_from_file(args[0], state)
	report["import_error"] = error
	if error != OK:
		finish("IMPORT_FAILED", 2)
		return
	# All authored keys and sample instants lie on the existing 24 fps playback grid.
	var scene := doc.generate_scene(state, 24.0)
	if not scene:
		finish("IMPORT_FAILED", 2)
		return
	root.add_child(scene)
	var meshes := scene.find_children("*", "MeshInstance3D", true, false)
	var players := scene.find_children("*", "AnimationPlayer", true, false)
	if meshes.size() != 1 or players.size() != 1:
		finish("UNSUPPORTED_SCENE", 2)
		return
	instance = meshes[0]
	skeleton = instance.get_node_or_null(instance.skeleton) as Skeleton3D
	if not skeleton or not instance.skin or instance.skin.get_bind_count() != 2:
		finish("BINDING_FAILED", 2)
		return
	var roots := skeleton.get_parentless_bones()
	if skeleton.get_bone_count() != 2 or roots.size() != 1 or not instance.mesh or instance.mesh.get_surface_count() != 1:
		finish("UNSUPPORTED_SCENE", 2)
		return
	var child := skeleton.get_bone_children(roots[0])[0]
	var skin_ref := instance.get_skin_reference()
	if not skin_ref or skin_ref.get_skin() != instance.skin:
		finish("BINDING_FAILED", 2)
		return
	report["renderer_skeleton_valid"] = skin_ref.get_skeleton().is_valid()
	var arrays := instance.mesh.surface_get_arrays(0)
	var vertices: PackedVector3Array = arrays[Mesh.ARRAY_VERTEX]
	if vertices.size() != 160 or arrays[Mesh.ARRAY_BONES].size() != 640 or arrays[Mesh.ARRAY_WEIGHTS].size() != 640:
		finish("UNSUPPORTED_SURFACE", 2)
		return
	report["vertices"] = vertices.size()
	report["binds"] = instance.skin.get_bind_count()
	var player: AnimationPlayer = players[0]
	player.callback_mode_process = AnimationMixer.ANIMATION_CALLBACK_MODE_PROCESS_MANUAL
	var root_rest := skeleton.get_bone_global_rest(roots[0])
	var child_rest := skeleton.get_bone_global_rest(child)
	var rest_basis := child_rest.basis
	var axis := rest_basis.y.normalized()
	var distal := (child_rest.origin - root_rest.origin).normalized()
	var use_bake := DisplayServer.get_name() != "headless"
	for clip: String in [STRIKE, IDLE]:
		if not player.has_animation(clip):
			finish("PLAYBACK_FAILED", 2)
			return
		player.play(clip)
		var rest := PackedVector3Array()
		var tip := 0
		for i in TIMES.size():
			var delta: float = 0.0 if i == 0 else TIMES[i] - TIMES[i - 1]
			player.advance(delta)
			# Let Skeleton3D flush the registered Skin palette to the rendering server.
			await process_frame
			await process_frame
			var pose := rest_basis.inverse() * skeleton.get_bone_global_pose(child).basis
			var direction := pose * Vector3.BACK
			var bone_deg := rad_to_deg(atan2(direction.x, direction.z))
			var reconstructed := reconstruct(arrays)
			if reconstructed.size() != vertices.size():
				finish("BINDING_FAILED", 2)
				return
			var points := reconstructed
			var sample := {"clip": clip, "time_s": player.current_animation_position, "bone_deg": bone_deg}
			if use_bake:
				var mesh := instance.bake_mesh_from_current_skeleton_pose()
				if not mesh or mesh.get_surface_count() != 1:
					finish("ENGINE_BAKE_UNAVAILABLE", 3)
					return
				points = mesh.surface_get_arrays(0)[Mesh.ARRAY_VERTEX]
				if points.size() != vertices.size():
					finish("ENGINE_BAKE_UNAVAILABLE", 3)
					return
				report["engine_bake"] = "MEASURED"
				sample["max_bake_vs_reconstructed_m"] = max_distance(points, reconstructed)
			if OS.is_stdout_verbose():
				print(JSON.stringify({"clip": clip, "time_s": sample.time_s,
					"points": Array(points).map(func(p): return [p.x, p.y, p.z]),
					"reconstruction": Array(reconstructed).map(func(p): return [p.x, p.y, p.z])}))
			if i == 0:
				rest = points
				for v in range(1, rest.size()):
					if rest[v].dot(distal) > rest[tip].dot(distal):
						tip = v
			var before := (rest[tip] - child_rest.origin).slide(axis)
			var after := (points[tip] - child_rest.origin).slide(axis)
			var skin_deg := rad_to_deg(before.signed_angle_to(after, axis))
			sample["tip_deg"] = skin_deg
			var distance := max_distance(points, rest)
			sample["max_displacement_m"] = distance
			report.samples.append(sample)
			var angle: float = ANGLES[clip][i]
			if (not skeleton.get_bone_global_pose(roots[0]).is_equal_approx(root_rest)
					or absf(sample.time_s - TIMES[i]) > 0.00001
					or not is_finite(bone_deg) or absf(bone_deg - angle) > 0.1):
				finish("PLAYBACK_FAILED", 2)
				return
			# Farthest distal vertex sweeps the chord of the 4.59 m calibrated radius.
			var chord := 2 * 4.59 * sin(deg_to_rad(absf(angle)) / 2)
			if (not is_finite(skin_deg) or absf(skin_deg - angle) > 0.1
					or not is_finite(distance) or absf(distance - chord) > 0.005
					or sample.get("max_bake_vs_reconstructed_m", 0.0) > 0.0001):
				finish("BINDING_FAILED", 2)
				return
	finish("PASS_ENGINE_BAKE" if use_bake else "PASS_HEADLESS_RECONSTRUCTION", 0)
