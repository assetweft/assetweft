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

## AW-026:A Independent VFX inspection

`checks::inspect_vfx_file` applies `read_bounded` separately to the JSON spec
and optional PNG. Unknown JSON fields and enum values are input errors.
The spec contains:

- `vfx_mode` (`parametric`, `flipbook`, or `hybrid`) and `authoring_state`
  (`native_editable` or `baked`). `requires_native_editable` defaults to
  false. When true, baked VFX fails and retains its declared state in the report.
- `timing` with positive finite `fps`, positive integer `frame_count`,
  integer `duration_ms`, boolean `loop_cycle`, and `blend_mode` (`alpha`,
  `additive`, or `premultiplied`). The signed duration error against
  `frame_count * 1000 / fps` is rounded to G-DET-1 micro-units and must be
  within 1 ms.
- A nonempty `parameters` array of `name`, `value`, `min`, and `max`.
  Names must be unique after trimming and nonblank. G-DET-1 canonical floats
  reject non-finite numbers and normalize signed zero. Values must be within
  the inclusive declared bounds.
- `flipbook` with `frame_width`, `frame_height`, `columns`, `rows`, and
  `require_transparency`. Flipbook and hybrid modes require the grid and PNG.
  Parametric mode omits both. The static 8-bit sprite decoder applies its
  existing dimension and memory limits. The grid must exactly match PNG
  dimensions and hold at least `frame_count` frames. Required transparency
  needs alpha support and at least one pixel with alpha below 255.

The report preserves the spec, decoded atlas facts, ordered violations, and
separate SHA-256 digests of the original JSON and PNG bytes. The PNG digest
is null when absent. `visual_engine_status` is always `NOT_RUN`.

## AW-027:A Local WAV inspection

`checks::inspect_audio_file` applies `read_bounded` independently to the WAV
and optional JSON spec. RIFF size matches the file length. A single supported
`fmt ` chunk precedes a single `data` chunk with at least one complete frame.
Other chunks are skipped by their declared size and RIFF padding. Supported
formats are PCM 8/16/24-bit and IEEE float32. Block alignment and byte rate
must match the declared positive channel count and sample rate.

PCM samples are normalized by their signed full-scale divisor, with 8-bit
samples first centered at 128. Float samples must be finite and representable
in G-DET-1 micro-units. `pcm_sha256` hashes canonical signed little-endian
micro-units in interleaved sample order without headers. Duration is reported
in milliseconds rounded to the nearest microsecond. Duration constraints use
the exact frame-count/rate ratio. Peak and RMS use the decoded values across
all channels. A sample at or beyond unit amplitude counts as clipped.

An optional spec checks `expected_channels`, `expected_sample_rate_hz`, inclusive
`min_duration_ms` / `max_duration_ms`, clipping and silence. Its `loop_region`
uses zero-based frames with an exclusive `end_sample`. The reported seam
metrics are the maximum per-channel step from the last frame to the first and
the maximum absolute endpoint amplitude. Exceeding either finite non-negative
limit produces `LoopBoundaryClick`. An invalid frame range or unknown spec
field is an input error. `transcript_text` is reported as supplied and
`chars_per_sec` counts Unicode scalar values over the exact duration.
`asr_playback_status` is always `NOT_RUN`.

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

## AW-025:A face and attachment inspection

The adapter reads each GLB/spec file with the same byte limit. The spec accepts
`face_mode`, `expected_expressions`, optional `eye_jaw_nodes` (each role is
optional), and `rigid_attachments`. Unknown spec fields are invalid. The GLB
parser resolves structure and indices. The adapter reads raw JSON name and
mesh `extras.targetNames` metadata because the shared SDK omits those features.

Morph target count sums slots across distinct node-bound meshes. Morph names
only count when they index actual primitive target slots. A relevant weight
channel or named eye/jaw TRS channel can match its clip name, channel
`extras.name`, or target node name. Each expected expression needs a matching
playable track. Tracks use embedded accessors with valid dense or sparse layouts, strictly ascending
finite times, correct output cardinality for their property and interpolation,
and two or more keys for playback evidence. The aggregate morph and track accessor budget is two
million entries. Specified eye/jaw roles resolve to distinct
nodes with finite rest transforms. `optional` allows an absent face when
no morph slots, names, or eye/jaw requirements exist. `not-applicable`
skips face acceptance and always evaluates attachments.

The socket and attachment names resolve uniquely. The part must descend
from the socket through rigid intermediaries. The part subtree cannot have
independent animation, skin binding, joint/weight attributes or morph
deformation. `expected_offset_m` is expressed in socket-local metres. Both
the local Euclidean drift and the world-space difference from the transformed
expected point must fit `max_drift_m`. G-DET-1 canonicalizes the coordinates
and drift to millionths of a metre via ties-to-even. The hierarchy walk is
bounded to two million intermediate links. `BYTE` and `SEMANTIC` are declared
lock modes. Both use this placement check and the input has no prior lock
content for comparison.

The version-1 semantic report digest hashes canonical report JSON with the
digest field empty, then feeds its 32 SHA-256 bytes as numeric values to
G-DET-1's semantic digest. This fingerprints the measured report. Visual
quality and engine playback remain `NOT_RUN`.

## AW-026:B Action presentation binding

`checks::inspect_action_presentation_file` reads one bounded JSON spec. The
spec connects AW-023:A motion phases to declared AW-026:A VFX and AW-027:A
sound cues on the same action clock. All fields are required and unknown fields
are rejected at every object level.

- `clip_duration_ms` is positive. `phases` contains `charge_ms`, `release_ms`,
  and `recovery_ms` as ordered, nonempty half-open intervals. Adjacent phases
  share an endpoint, and the intervals lie within the clip.
- `bindings` is nonempty. Each binding supplies a string `id`, `kind` (`vfx`
  or `sfx`), `phase` (`charge`, `release`, or `recovery`), `trigger_ms`,
  and `duration_ms`. Times and durations use exact unsigned integer
  milliseconds. Each trigger lies inside its declared phase.
- `cancel_cleanup_policy` accepts `stop_and_clear` and `fade_out`.
  `persist_leak` reports `UncleanCancelCleanup`.
- `event_source` is either `fixed_timeline` or `authoritative_hit`. The
  required boolean `claims_gameplay_hit` must be false for fixed cues.
  Violations report `HitEventConflation`.

`presentation_sha256` hashes the canonical typed spec JSON, preserving binding
array order and exact integer milliseconds. Its 32 SHA-256 bytes are passed as
numeric values to G-DET-1's semantic digest. Contract failures report their
violations and the same digest scheme. The inspector evaluates declared bindings
and leaves engine playback, cancellation behavior, and hit detection unexecuted
(`engine_playback_status: "NOT_RUN"`).
