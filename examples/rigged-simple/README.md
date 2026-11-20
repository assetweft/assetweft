# Rigged Simple in-place strike

`RiggedSimple.glb` is the original **Rigged Simple** by Cesium, © 2017, distributed under CC BY 4.0. The upstream license notice accompanies it. The source SHA-256 is `3a79dabb67bb0cd598a18d08b954d9d357c27c30672f82ef5d3f4e7fe6ca3401`. [Upstream model and attribution](https://github.com/KhronosGroup/glTF-Sample-Assets/tree/f0920f5b32d172a30efe3f9257e965918831e6ea/Models/RiggedSimple) · [License notice](LICENSE.md)

From the repository root, write the derivative to a new file under an existing directory.

```sh
cargo run --locked -p assetweft-cli -- motion-strike examples/rigged-simple/RiggedSimple.glb --out strike.glb
```

Open the output in a glTF animation player and select **AssetWeft_InPlaceStrike**. The original animation is still the first clip. The added clip holds the rest pose for 0.25 s, bends the distal cylinder segment through 90° by 0.5 s, and recovers by 1 s. This two-joint cylinder supports a single child rotation around local Y. Node 3 (`Bone`) anchors the skeleton in world space. Node 4 (`Bone.001`) drives the weighted body through its existing skin. The clip has one rotation channel and keeps the scene root, armature, root joint and child translation fixed. It has no limbs, contact targets or locomotion.

The Rust command runs without Blender and reports the source and output SHA-256. To measure playback separately, run this optional check with Blender 5.1.2.

```sh
blender --background --factory-startup --disable-autoexec --python-exit-code 1 \
  --python examples/rigged-simple/playback.py -- examples/rigged-simple/RiggedSimple.glb strike.glb
```

The check reopens the derivative, selects each imported action in turn and evaluates the scene at seven frames at 24 fps. It checks the distal bone angle, maximum skinned-vertex displacement and stationary root during idle, attack, interpolation and recovery. The preserved original action must fail the same strike curve. Original scene data, upstream animation and BIN prefix are compared with the licensed input. This profile covers one cylinder. F0, IK, engine playback and multiple actors remain unmeasured.
