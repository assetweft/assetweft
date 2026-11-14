# Assurance bounded validation

Assurance owns the identical validation report, role, status, limits and source
values re-exported at `validation::*`. The legacy `validate_file` and
`validate_bytes` functions assemble the concrete adapters and invoke application
use cases; `read_bounded` remains the same concrete reader API for geometry/rig.
No consumer changes, schema, persistence, new format or dependencies are added.

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

Domain report policy alone decides component presence, role satisfaction and
NOT_RUN. Format failure leaves role NOT_RUN with no inferred missing components.
Unbound skin and orphan animation do not satisfy rig/motion. Parse success and
role success do not imply numerical/visual/engine quality or verified rights.
Checker version, scope, NOT_RUN list, report field order and diagnostics remain.
Geometry and Rig report types now belong to `geometry_domain` and `rig_domain`;
legacy modules re-export the identical types and only assemble concrete adapters.
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
Geometry read errors retain the original ValidationError payload at the legacy
composition boundary. Core capabilities expose no Path/io/SDK/concrete errors.
Unsupported external/sparse/viewless/extension/multiple-set data stays NOT_RUN;
invalid format never produces numeric PASS. No pose/rest/semantic/engine claims.

Remaining migration debt: internal consumers still use legacy compatibility paths, other Assurance
measurement/export/engine capabilities and broader context migration are outside
this bounded card. Full affected CLI/geometry/rig execution, public current-SHA
CI, independent review, protected merge and postmerge checks belong to root.

## Public check composition (W6C1)

`assurance::checks` now owns the existing concrete `validate_file`,
`validate_bytes`, `inspect_geometry_file`, `inspect_geometry_bytes`,
`inspect_rig_file` and `inspect_rig_bytes` entry functions. It assembles only
FileInput and the existing GLB adapters and calls the unchanged application
use cases. `GeometryError` is defined once here as a concrete composition
error, preserving its Debug derive and Read(ValidationError)/Invalid(String)
payloads; no concrete IO error is introduced into the pure core.

This supersedes the earlier description of legacy modules assembling adapters:
`validation`, `geometry` and `rig` explicitly re-export the same function items
(with inspect aliases) and the same public values, including GeometryError.
`validation::read_bounded` and `ValidationError` remain unchanged exports.
No adapter implementation is made public. Internal CLI/Application/staging and
example consumer import migration remains a subsequent batch; this entry move
does not complete whole-DDD migration or improve F0, rights, numerical scope,
engine, provider, production or authorization evidence.

## Canonical consumer exports (W6C2)

`assurance::{read_bounded, ValidationError}` publicly re-exports the SAME
file_input items as legacy `validation`; the concrete module stays crate-private.
Application, CLI, Production specification/staging and the rig_blender example
consume Assurance values and existing checks directly. CLI specification values
come from Production; Application executor composition uses Execution and
Consumption owners. Legacy public compatibility exports remain identical items.
This supersedes only the preceding deferred consumer migration statement.
