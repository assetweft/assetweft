# Export matched diagnostic GLBs from the verified Rigged Simple derivative.
# Caller: local scratch project. Both exports take the same Godot round trip.
extends SceneTree

const DERIVATIVE = "667d8bfccdeb68d64c9668ff54ab0af907aa240988a3d488f49ff7b43152b09d"

func _initialize() -> void:
	call_deferred("run")

func run() -> void:
	var args := OS.get_cmdline_user_args()
	if args.size() != 2 or args[1] not in ["normal", "root-only"]:
		printerr("Expected the --with-idle GLB path and normal or root-only")
		quit(2)
		return
	var output := "res://%s.glb" % args[1]
	if FileAccess.get_sha256(args[0]) != DERIVATIVE or FileAccess.file_exists(output):
		printerr("Expected the pinned input and a fresh output path")
		quit(2)
		return
	var doc := GLTFDocument.new()
	var source := GLTFState.new()
	var error := doc.append_from_file(args[0], source)
	if error != OK:
		printerr("Input import error ", error)
		quit(2)
		return
	var scene := doc.generate_scene(source, 24.0)
	if not scene:
		quit(2)
		return
	root.add_child(scene)
	var mesh: MeshInstance3D = scene.find_children("*", "MeshInstance3D", true, false)[0]
	var arrays := mesh.mesh.surface_get_arrays(0)
	if args[1] == "root-only":
		for i in arrays[Mesh.ARRAY_VERTEX].size():
			# A valid normalized weight of one on the stationary root joint.
			for j in 4:
				arrays[Mesh.ARRAY_BONES][4 * i + j] = 0
				arrays[Mesh.ARRAY_WEIGHTS][4 * i + j] = 1.0 if j == 0 else 0.0
	var rebuilt := ArrayMesh.new()
	rebuilt.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arrays)
	rebuilt.surface_set_material(0, mesh.mesh.surface_get_material(0))
	mesh.mesh = rebuilt
	var state := GLTFState.new()
	state.copyright = source.copyright + " Godot diagnostic re-export: " + args[1]
	error = doc.append_from_scene(scene, state)
	if error == OK:
		error = doc.write_to_filesystem(state, output)
	print(JSON.stringify({"export_error": error, "variant": args[1], "output": output}))
	quit(0 if error == OK else 2)
