# Rigged Simple strike and looping idle

`RiggedSimple.glb` is the original **Rigged Simple** by Cesium, © 2017, distributed under CC BY 4.0. The upstream license notice accompanies it. The source SHA-256 is `3a79dabb67bb0cd598a18d08b954d9d357c27c30672f82ef5d3f4e7fe6ca3401`. [Upstream model and attribution](https://github.com/KhronosGroup/glTF-Sample-Assets/tree/f0920f5b32d172a30efe3f9257e965918831e6ea/Models/RiggedSimple) · [License notice](LICENSE.md)

From the repository root, write the derivative to a new file under an existing directory.

```sh
cargo run --locked -p assetweft-cli -- motion-strike examples/rigged-simple/RiggedSimple.glb --out strike.glb
```

Open the output in a glTF animation player and select **AssetWeft_InPlaceStrike**. The original animation is still the first clip. The added clip holds the rest pose for 0.25 s, bends the distal cylinder segment through 90° by 0.5 s, and recovers by 1 s. This two-joint cylinder supports a single child rotation around local Y. Node 3 (`Bone`) anchors the skeleton in world space. Node 4 (`Bone.001`) drives the weighted body through its existing skin. The clip has one rotation channel and keeps the scene root, armature, root joint and child translation fixed. It has no limbs, contact targets or locomotion.

Add `--with-idle` to include **AssetWeft_Idle**. With the player set to repeat, the distal joint sways from rest to +8°, then −8°, then rest each second. The fixed-root sway requires a mesh bound to a single skin with two directly connected joints. An incompatible hierarchy returns `NOT_RUN` with the structural reason. A topology match still requires the exact verified source before either clip is written.

The Rust command runs without Blender and reports the source and output SHA-256. To measure playback separately, run this optional check with Blender 5.1.2.

```sh
blender --background --factory-startup --disable-autoexec --python-exit-code 1 \
  --python examples/rigged-simple/playback.py -- examples/rigged-simple/RiggedSimple.glb strike.glb
```

For an output generated with `--with-idle`, also append `--with-idle` after the output path in the verifier command. It compares the original scene, skin, animation and BIN prefix against the source, then measures the signed child and skinned-tip angles, root transform and mesh displacement. It samples the strike at seven frames and requires the upstream action to fail the strike curve. For idle it plays a repeating Blender NLA strip through frames 0–48 at 24 fps and compares all 160 evaluated vertices across corresponding frames of both periods. The measurements cover this cylinder in Blender.

## Godot playback

Use official **Godot 4.7.2 stable** and a fresh scratch project. Copy
`examples/godot-static/project.godot`, its `malformed.glb`, and the two
`godot_*.gd` scripts here into scratch. Generate `strike.glb` there using
the CLI command above with `--with-idle`. Its SHA-256 must be
`667d8bfccdeb68d64c9668ff54ab0af907aa240988a3d488f49ff7b43152b09d`.
Keep the original model and its license. These probes are for this fixture.

Run the following from scratch. Set `GODOT` to the verified binary's absolute
path. On macOS leave the signed bundle intact. The clean child environment
below uses the scratch working directory for Godot 4.7.2's relative data/cache
fallback and sets the engine log explicitly. Use a 60-second timeout per run.

```sh
env -i PATH=/usr/bin:/bin TERM=dumb "$GODOT" --headless --path . \
  --log-file "$PWD/headless.log" --script godot_playback.gd -- res://strike.glb
for variant in normal root-only; do
  env -i PATH=/usr/bin:/bin TERM=dumb "$GODOT" --headless --path . \
    --log-file "$PWD/export-$variant.log" --script godot_binding_control.gd -- res://strike.glb "$variant"
done
```

The headless run advances the imported AnimationPlayer through six instants
in each one-second clip. It checks the fixed root, signed child and distal
tip angles, and displacement over all 160 vertices. Scene generation uses
24 fps so every sampled time, including the hold at 0.25 s, lands on the
same grid as the Blender probe. `PASS_HEADLESS_RECONSTRUCTION` measures live
bone poses and computes skinning in the probe from the imported binds and
weights. Godot's headless dummy renderer cannot bake a mesh.

To read the engine's evaluated vertices, use the same scratch files on Linux
with Xvfb and Mesa software OpenGL. For each file below, set `asset` to its
filename and run this command. Inspect both the process exit and JSON status.

```sh
XDG_DATA_HOME="$PWD/data" XDG_CONFIG_HOME="$PWD/config" XDG_CACHE_HOME="$PWD/cache" \
LIBGL_ALWAYS_SOFTWARE=1 timeout 60 xvfb-run -a -s '-screen 0 640x480x24 -nolisten tcp' \
  "$GODOT" --display-driver x11 --audio-driver Dummy --rendering-method gl_compatibility \
  --path . --log-file "$PWD/$asset.log" --script godot_playback.gd -- "res://$asset"
```

| File | Expected exit and status | What it checks |
| --- | --- | --- |
| `strike.glb` | 0, `PASS_ENGINE_BAKE` | Original delivered motion derivative |
| `normal.glb` | 0, `PASS_ENGINE_BAKE` | Same motion after the control exporter round trip |
| `root-only.glb` | 2, `BINDING_FAILED` | Valid export/import, moving child, mesh weighted entirely to stationary root |
| `malformed.glb` | 2, `IMPORT_FAILED` | Native importer rejects the file before playback |

The renderer path calls `MeshInstance3D.bake_mesh_from_current_skeleton_pose()`
and compares each returned vertex with the weighted reconstruction at each
sample. Add Godot's `--verbose` flag to retain the full position arrays.
These are model-space CPU bake measurements using the registered renderer
skeleton. Raster appearance and GPU vertex output are unmeasured.
