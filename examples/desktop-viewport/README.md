# AW005 isolated native static Mesh experiment

M6 incoming adapter only; Tauri 2 / Three.js remain candidates. This does not close
AW005A (Skin/Morph/animation/resource coverage), AW005B, F0 or product-quality gates.
The root Rust workspace and five existing contexts are unchanged.

Input is exactly ../godot-static/triangle.glb (804 bytes, SHA256
44e32648c9f09f1ee221b89b7da61dd29aecf4227cf6c704dd73ca3ab89c93c1).
Read its fixture.json and README for the analytic TEST_FIXTURE / CC0-1.0 caller
declaration, metre coordinates, translation, scale and green material. This app
performs no rights verification. No new positive sample is supplied.

Use Node 24 and the repository Rust toolchain. In this directory run `npm ci
--ignore-scripts`, `npm run typecheck`, `npm run build`, then `cargo build --locked
--manifest-path src-tauri/Cargo.toml`. The isolated Cargo.lock deliberately fixes
the Tauri components to the official tauri-v2.9.3 release set; broad transitive
version resolution otherwise selected incompatible runtime/macros. Root Cargo
checks alone never compile this isolated app. Cargo caches, target, npm cache
and TMPDIR must point to an owned external task directory before local builds.

Launch the executable with AW_VIEWPORT_RUN pointing to a **nonexistent**, uniquely
owned directory. It explicitly creates a technical library through Application,
commits the fixed fixture to managed CAS, resolves its immutable identity, runs
Core validation and transports only admitted bytes over binary IPC. Empty and
malformed controls are owned files; UI cannot select arbitrary paths. No SQL,
filesystem/shell plugin, model network loader, picker or domain validation lives
in the UI. Renderer state is transient. The window has three explicit load/control
buttons; errors dispose the prior scene and display truthful Core rejection.

For macOS launch an owned bundle using `open -g --env AW_VIEWPORT_RUN=...`.
WebView data_directory is requested, but platform support and OS-owned cache
locations must be measured, never assumed isolated. Do not change OS settings.

After rendering, the owned run directory contains render.png, Core report and
renderer-observations.json. Run `node compare.mjs <render.png> <observations.json>`.
The dev-only comparator parses source GLB float positions and node transform,
projects through the fixed orthographic camera, and compares green pixel bounds, declared material and analytic triangle row spans
(including interior continuity). It supports only that exact fixture SHA and fixed camera. `--wrong-bounds` injects a failure; repeat correctly on the
same image for GREEN. Native screenshots use `--native` and recorded window/device
scale. The two-line status has reserved space and reaches its ready text before canvasRect
is measured; a new-source native run must capture fresh observations. Old recorded
rectangles are never corrected retrospectively.

Raw native screenshots remain unmodified. The comparator reads PNG iCCP/sRGB
metadata before decoding pixels; untagged canvas exports are interpreted as sRGB,
while untagged native captures are rejected. Supported ICC bytes are pinned in
compare.mjs: the observed capture Display P3, macOS Display P3, and macOS sRGB.
Other profiles are rejected, rather than guessed from a profile name. For Display
P3 only, on macOS with exactly sips-316 and the pinned system sRGB profile, pass
`--canonical-dir <owned-output-directory>`. The gate creates an exclusive derivative
named by raw SHA256, then uses Apple's ColorSync via
`sips --matchToWithIntent '/System/Library/ColorSync/Profiles/sRGB Profile.icc' relative <derivative>`.
This is dev-only comparison preprocessing, not a required product runtime. The
local primary reference is `man sips` (sips-316, checked 2026-11-16). Output discloses
raw/profile/target/derivative hashes, version, command and receipt. The canonical
image is a comparison derivative, never a raw capture; keep it alongside its raw
source. Existing output is rejected without overwriting. Bounds tolerance remains
3*scale pixels; material thresholds remain unchanged. A profile-corrected old
capture can still fail geometry because its original canvasRect was stale. Timing is a single warm renderer
load plus 60 requestAnimationFrame intervals, not a benchmark minimum. Parent RSS
sampling excludes unattributed WebKit helper memory. Windows/Linux native runtime
render acceptance is NOT_RUN until actual windows are captured independently.

No production deployment, remote publication, framework freeze or E4 claimed.
