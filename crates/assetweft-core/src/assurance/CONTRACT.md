# Assurance bounded validation

Assurance owns the identical validation report, role, status, limits and source
values re-exported at `validation::*`. The public concrete entry functions live in
`assurance::checks`; legacy `validation`/`geometry`/`rig` re-export those same
items, and `read_bounded` remains the same concrete reader API for geometry/rig.
No schema, persistence, new format or dependencies are added.

Application depends only on domain and two purpose-owned capabilities:
`BoundedInput` reads a bounded input selected by the composition boundary, with
an associated error preserved unchanged; `StructureCheck` checks exact bytes
and returns their digest, length and either semantic component facts or the
original format diagnostic. No Path, fs/io handle or gltf SDK type enters core.
These internal ports are not public APIs or generic repository abstractions.

`file_input` retains the original ValidationError type, variants, derives and
Display/Error implementations. Metadata is checked before opening; the read
also takes at most saturating limit+1 and rechecks size, preserving limit/error
precedence. `gltf_checks` retains container validation and gltf 1.4.1 parsing,
computes SHA-256, and translates bound mesh/skin/joint relationships to facts.
It never resolves external URI/image resources or evaluates role acceptance.
The shared GLB parser checks `EXT_texture_webp` declarations and image references
before adapting texture sources for gltf 1.4.1. Only the validated WebP entry is
removed from the SDK's required extensions for parsing. Geometry reports the
original declarations and selected image indices. Hashes and byte counts use
the original GLB. This checks declarations and resource references without
decoding image payloads.

Domain report policy alone decides component presence, role satisfaction and
NOT_RUN. Format failure leaves role NOT_RUN with no inferred missing components.
Unbound skin and orphan animation do not satisfy rig/motion. Parse success and
role success do not imply numerical/visual/engine quality or verified rights.
Checker version, scope, NOT_RUN list, report field order and diagnostics remain.
Geometry and Rig report types belong to `geometry_domain` and `rig_domain`;
legacy modules re-export the identical types, and only `assurance::checks`
assembles concrete adapters.
Geometry application reads bounded input, validates the same bytes through the
existing format capability, invokes `GeometryStructure`, then applies pure report
policy. SDK mapping preserves matrix/TRS, original indices and unresolved URI
strings, including data URI. UNKNOWN declarations and format conventions remain
separate; this is a STRUCTURE summary, not geometry value computation.

Rig application owns bounded-read and validation-before-numeric sequencing.
`RigNumeric` is the actual GLB traversal/streaming capability: SDK readers and BIN
slicing remain in `rig_gltf`; it calls the single pure forest, joint/attribute,
layout, affine-matrix and per-vertex policy implementations in `rig_domain`.
No large matrix/weight arrays are cloned. Original check order, messages, actual
mesh-node skin binding selection and early returns remain. Layout policy checks
subtraction/multiplication/addition, relative and absolute component alignment,
vertex four-byte requirements, and forbids any IBM stride without inventing
16-byte alignment. Quantized raw sums stay exact; float tolerance counts nonzero
influences; zero-weight joint indices still require range validity.
Geometry read errors retain the original ValidationError payload at the
`assurance::checks` composition boundary. Core capabilities expose no
Path/io/SDK/concrete errors.
Unsupported external/sparse/viewless/extension/multiple-set data stays NOT_RUN;
invalid format never produces numeric PASS. No pose/rest/semantic/engine claims.
The validated WebP extension only affects texture sources, so it does not gate
rig numeric checks.

Unimplemented capabilities are unchanged: other Assurance measurement/export/
engine capabilities and engine observation remain future work. CLI/geometry/rig
execution, current-SHA CI, independent review, protected merge and post-merge
checks are root-owned.

## Public composition and consumer exports

`assurance::checks` owns the existing concrete `validate_file`, `validate_bytes`,
`inspect_geometry_file`, `inspect_geometry_bytes`, `inspect_rig_file` and
`inspect_rig_bytes` entry functions. It assembles only FileInput and the existing
GLB adapters and calls the unchanged application use cases. `GeometryError` is
defined once here as a concrete composition error, preserving its Debug derive and
Read(ValidationError)/Invalid(String) payloads; no concrete IO error enters the
pure core, and no adapter implementation is made public. `validation`, `geometry`
and `rig` explicitly re-export the same function items (with inspect aliases) and
the same public values, including GeometryError; `validation::read_bounded` and
`ValidationError` remain unchanged exports.

`assurance::{read_bounded, ValidationError}` publicly re-exports the SAME
file_input items as legacy `validation`; the concrete module stays crate-private.
Application, CLI, Production specification/staging and the rig_blender example
consume Assurance values and existing checks directly. CLI specification values
come from Production; Application executor composition uses Execution and
Consumption owners. Legacy public compatibility exports remain identical items,
and this entry move does not complete whole-DDD migration or improve F0, rights,
numerical scope, engine, provider, production or authorization evidence.


`rig_blender::measure_rigged_simple` is a separate optional measurement on one pinned GLB.
It requires the existing numeric rig checks to pass, then uses a caller-selected local Blender
and checks its evaluated coordinates against the preset and disabled-driver reference.
The [CLI profile](../../../assetweft-cli/README.md#rigged-simple-bend-measurement) documents
the asset identity, license, pose, units and tolerance. Generic rig inspection still reports
pose deformation as NOT_RUN.

## AW-028:A Static kit inspection

`checks::inspect_static_kit_file` reads a GLB v2 and a JSON spec under the same
per-file byte limit. It accepts embedded BIN geometry and buffer-view images
(including the existing `EXT_texture_webp` declaration checks). It measures
triangle lists, strips, and fans with dense float positions and optional dense
unsigned indices. External resources and sparse geometry are outside this
inspection scope. Geometry decoding is capped at two million combined position
and index entries, and bounds at two million instanced positions.

The spec requires `kind` (`architecture` or `foliage`), positive
`expected_unit_scale` and `scale_tolerance`, `sockets`, and
`allow_non_watertight_foliage`. All mesh-node world-space basis lengths must
match the expected scale within tolerance on each axis. Each uniquely named
socket declares `position` (XYZ in metres) and nonnegative `seam_tolerance_m`.
Its case-sensitive name must resolve to exactly one GLB node. The Euclidean
distance between the node's world-space origin and the expected point must be
within tolerance. Measures and tolerances use G-DET-1 micro-unit rounding.

The report counts stored primitives, vertices and triangles across mesh
definitions. Its world-space bounds cover positions in all mesh nodes, including
instances. Edge checks weld equal quantized positions within each mesh across
primitives. A closed shell requires two oppositely oriented incidences per
edge and rejects triangles with coincident welded vertices. This is an edge
check. Self-intersections and volume are outside its scope. Only a `foliage`
spec can enable `allow_non_watertight_foliage`. Both single- and double-sided
open cards then pass the topology policy, with edge counts and material alpha
modes still reported.

An optional `budget_before` supplies `vertex_count`, `triangle_count`, and
`texture_bytes`. Measured texture bytes sum each distinct embedded image
buffer view. The optimization record gives before and after counts and signed
after-minus-before triangle and texture-byte deltas (null without a baseline).
Deltas outside the signed 64-bit range use exact decimal strings.
`visual_quality_status` is always `NOT_RUN`. This operation only inspects files.

## G-DET-1 DeterminismPolicy

Canonical floats reject NaN and infinities and map both signed zeros to `+0.0`.
Vectors and matrices preserve entry order. Quantization multiplies by `1_000_000`,
rejects values outside the signed `i64` range, and rounds ties to even. Integer
micro-units are authoritative for byte encoding.

glTF quaternions use `[x, y, z, w]` order. Reject non-finite components and
squared norms that are non-finite or at most `1e-24`. Normalize with separate
Rust multiply and add operations and `f64::sqrt`, then quantize. The first
nonzero micro-unit in `w, x, y, z` order must be positive.

`canonical_semantic_bytes` encodes ordered signed micro-units as eight bytes each
in little-endian order. `semantic_sha256_hex` hashes the ASCII tag
`assetweft:semantic:1` followed by one NUL byte and the canonical bytes, and
returns lowercase hex. Pass rotations through `canonical_quaternion` before
hashing so equivalent signs produce the same digest. Component-level SEMANTIC
execution remains a caller responsibility.

## AW-024:A Sprite sheet inspection

`checks::inspect_sprite_file` reads the PNG and manifest independently with
`read_bounded`. It accepts static 8-bit RGBA, RGB, grayscale, grayscale+alpha
and indexed PNGs (with or without palette transparency). Each side is limited to
4096 pixels, total area to 4,194,304 pixels and decoder scratch allocations to
a 16 MiB decoder budget. APNG, other sample/index depths, truncated PNGs and
bytes after IEND are input errors. The inspection uses raw decoded samples and
ignores embedded color profiles and text.

The manifest declares positive grid dimensions under `sheet` with
`frame_width`, `frame_height`, `columns` and `rows`. The grid must exactly fill the
PNG. Ordered `frames` use contiguous zero-based `index` values and distinct
zero-based `col` and `row` slots. Unused slots are allowed. Each `anchor_x` and
`anchor_y` is in [0, 1], with signed zero canonicalized by G-DET-1. The
`looping` object declares `loop_cycle` and inclusive `start_frame` and
`end_frame` within the sequence, including when `loop_cycle` is false.

Frame hashes are lowercase SHA-256 of the frame's decoded RGBA8 bytes in
top-to-bottom, left-to-right order. Grayscale is replicated across RGB and
missing alpha becomes 255. RGB samples behind zero alpha are retained.
Per-frame alpha presence reflects the source alpha channel or PNG transparency
declaration. `transparent_pixels` counts alpha below 255 and `opaque_pixels`
counts alpha equal to 255. An empty frame has alpha zero throughout.
`has_transparency` covers the full sheet, including unused slots.
Duplicate groups list equal hashes, with ascending frame indices and groups
ordered by their first index. Duplicate and empty frames are observations.
`semantic_quality_status` remains `NOT_RUN`.

## AW-024:B card template layout check

`checks::check_card_template_file` reads the template and optional illustration
under the same per-file byte limit. The strict JSON contract accepts `kind`
(`ui`, `card`, `tile`), `canvas` (`width_px`, `height_px`, each 1–4096),
`illustration_slot`, `text_boxes` (1–1024), and the optional boolean
`allow_text_over_illustration` (defaults to `true`).

- The slot requires a nonblank `slot_id`, unsigned pixel `x`, `y`, `width`,
  and `height`, `editable_independently: true`, and `fit_mode` of `contain`
  or `cover`. `art_asset_sha256` is optional reference metadata.
- Each text box requires a unique nonblank `box_id`, the same pixel rectangle
  fields, `text`, `font_px`, `line_height_px`, `max_chars_per_line`, and
  `max_lines`. All rectangles have positive area and stay inside the canvas.
  Text boxes cannot overlap one another. Edge contact is allowed. Setting the
  overlay flag to `false` also disallows text box overlap with the slot.
- Line height must be at least the positive font size. Both text limits must be
  positive and `max_lines * line_height_px` must fit the box height. Each
  newline starts a line, including blank lines. Other whitespace separates
  words and is counted as one scalar between words on a line. Words wrap
  greedily. A word that exceeds the scalar limit fails.
- `template_layout_sha256` hashes the canonical semantic bytes of canvas and
  slot geometry followed by typed JSON with the `card_layout_v1` tag, slot
  settings, and text boxes sorted by ID.
  The art reference, PNG bytes, template kind, and overlay policy are excluded.
  An optional PNG uses the sprite decoder and limits. Its raw-file SHA-256 and
  dimensions are reported independently. Its centered fitted rectangle uses
  signed pixel numerators and a shared positive denominator. Cover reports the
  full scaled rectangle before clipping to the slot.

The checker measures declared constraints. Font rendering and visual quality
remain `NOT_RUN`.
