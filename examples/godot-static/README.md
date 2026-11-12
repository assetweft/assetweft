# Godot static import trial (AW031:A)

Requires **Godot 4.7.2 standard**, no .NET, Python, npm or Blender consumer runtime.
This isolated example uses Godot's mature GLTFDocument importer and instantiates
its actual generated scene. It checks transformed vertex bounds, mesh/index
counts, pure green PBR material and absence of textures. No external resources
or scripts are present in either fixture. It is a fixed integration probe,
not a general importer for untrusted files.

`fixture.json` states the independently specified analytic geometry and units.
The triangle and malformed negative are original TEST_FIXTURE data dedicated to
the public domain under CC0-1.0 (https://creativecommons.org/publicdomain/zero/1.0/).
They are neither user Rig F0 nor AI output. A triangle deliberately isolates scale,
translation and material import without making geometry quality claims.

Set `GODOT` to your official 4.7.2 executable. Copy this directory to a fresh
scratch project so generated `.godot` files stay outside source, then run:

```sh
"$GODOT" --headless --version
"$GODOT" --headless --path /path/to/scratch-project --import
"$GODOT" --headless --path /path/to/scratch-project --script verify.gd
"$GODOT" --headless --path /path/to/scratch-project --script verify.gd -- --wrong-bounds
"$GODOT" --headless --path /path/to/scratch-project --script verify.gd
"$GODOT" --headless --path /path/to/scratch-project --script verify.gd -- --malformed
```

Apply a 120-second process timeout to each command. Success is exit 0 **and**
parsed JSON `status=PASS`; wrong bounds and malformed input must exit 2 with
`BOUNDS_MISMATCH` and `IMPORT_FAILED`, respectively. Editor import can log an
error yet exit 0 for malformed.glb: it alone is never acceptance. GLTFDocument
reimports from source in each verifier process, independently of editor caches.
Repeat in a second fresh scratch copy to check reproducibility.

No network is used by this project. Use only these constructed fixtures.
On macOS keep the signed app bundle intact; Godot's documented self-contained
`._sc_` marker keeps editor settings/data beside the executable. That marker
changes bundle sealing, so verify the original app signature first. Never
remove quarantine or disable Gatekeeper. Keep runtime/cache/logs in scratch.
The verified engine is MIT licensed; retain its LICENSE.txt and COPYRIGHT.txt
for third-party notices when redistributing it. No engine binary is shipped here.

Official references (checked 2026-11-12):
- https://godotengine.org/download/macos/
- https://docs.godotengine.org/en/4.7/tutorials/assets_pipeline/importing_3d_scenes/available_formats.html
- https://docs.godotengine.org/en/4.7/tutorials/io/data_paths.html
- https://godotengine.org/license/

Headless import is not render acceptance. Rendered appearance, collisions,
physics, Skin/Motion, Godot B/C/D, AI quality, Windows and production are NOT_RUN.
Public-SHA CI and independent review remain required for product acceptance.
