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
