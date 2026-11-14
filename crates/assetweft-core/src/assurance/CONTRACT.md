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
Pure numerical geometry/rig work remains in its existing modules.

Remaining migration debt: legacy geometry/rig consumers, other Assurance
measurement/export/engine capabilities and broader context migration are outside
this bounded card. Full affected CLI/geometry/rig execution, public current-SHA
CI, independent review, protected merge and postmerge checks belong to root.
