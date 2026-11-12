# assetweft CLI v1 (AW-033:A)

Build with `cargo build -p assetweft-cli --locked`; run `target/debug/assetweft`.
No daemon, Python, DB writes, model, shell, external resource resolution or remote execution.

```
assetweft [--call-id ID] inspect FILE.glb
assetweft [--call-id ID] validate FILE.glb [--role static-mesh|rigged-character|animated-character]
assetweft [--call-id ID] plan --request request.json --trusted-context context.json
```

Global `--max-bytes N` bounds each input separately (default and hard ceiling
16777216, minimum 1); uses the existing core `read_bounded`, including JSON.
Paths are OS paths and support spaces/Unicode. Files are read without mutation.

Every invocation emits exactly one compact JSON document and newline on stdout:
`schema_version: 1`, `command`, `call_id` (null if absent/unparsed), `status`,
`errors: [{code,field_path,message}]`, `results` (null on input errors).
Help/version are JSON results with status HELP/VERSION and exit 0, never plain
stdout. Argument diagnostics also go to stderr. Supply `--call-id` before invalid
arguments when correlating syntax errors: clap cannot recover tokens after the
first unrecognized argument. Valid invocations accept the global ID on either
side of the subcommand; help/version preserve it as well. No timestamps/random IDs.
Repeated calls with identical inputs produce identical stdout. OS I/O diagnostics
can vary with environment; they are not a cross-platform canonical digest.

`inspect` delegates static GLB structure inspection to core geometry. PASS means
only that inspection returned a report; unknown units stay UNKNOWN and the
report lists unexecuted checks. It does not render or read linked URIs.

`validate` delegates core bounded file and role checks (default static mesh).
PASS/0 covers these implemented checks only. Full semantics, provenance,
rendering and engines are explicitly NOT_RUN; this is not production acceptance.
Source is UNKNOWN, checker verification false; no provenance is inferred.

`plan` parses the core GenerationSpecRequest and TrustedContext DTOs separately
and delegates validation to generation_spec. The context file must be supplied
from the caller's predeclared contract, independently of the editable request.
The CLI cannot authenticate that context or authorization. A successful result is
DECLARATION_CONSISTENT/0, never aggregate PASS: execution is always NOT_RUN,
authenticity NOT_VERIFIED. Recipe describes scope only. Unknown costs stay null;
TEST_FIXTURE and authorization/engine declarations never imply real execution.

Exit mapping (CONTRACTS §10.5): 0 scoped checks pass / declaration consistent /
help/version; 1 core validation or other contract FAIL; 2 malformed arguments,
unreadable/oversized files, malformed JSON/GLB inspection or core INVALID_INPUT;
3 core INPUT_INCOMPLETE; 4 core LOCK_VIOLATION; 5 core UNSUPPORTED; 6 output I/O
INTERNAL. Core semantic missing fields use code 3; absent CLI syntax uses 2.
Multiple plan issue categories use priority 4,2,3,5,1; all issues remain visible.
Unimplemented execution does not become a failure or a successful execution.

Dependency decision (2026-11-12): exact clap 4.6.7, official
https://docs.rs/clap/4.6.7/clap/struct.Command.html and crates.io sparse index
https://index.crates.io/cl/ap/clap plus the published package Cargo.toml confirm
version, MIT OR Apache-2.0, MSRV 1.85 (workspace 1.97). Builder API features only:
std/help/usage/error-context; no derive, color, suggestions or platform runtime.
serde/serde_json reuse exact existing pins. No second schema engine. Geometry's
Serialize derives expose its existing report tree directly to this consumer;
no geometry or validation behavior changed. Black-box tests exercise that tree.

C2 is `cargo test -p assetweft-cli --locked` within existing workspace C1.
Synthetic fixtures demonstrate local behavior only; Windows/F0/AI/engines remain
NOT_RUN. Current-SHA CI and independent non-author review are separate root gates.

SEMANTIC lock declarations in both `locked_components` and independent
`declared_locks` require `canonicalization_id` and `canonicalization_version`
(nonblank opaque strings, compared verbatim). BYTE does not require them.
Old SEMANTIC declarations without these fields now intentionally return
`INPUT_INCOMPLETE` (exit 3); differing complete identities return
`LOCK_VIOLATION` (exit 4). Example identity fields:
`"canonicalization_id":"TEST_FIXTURE:rig","canonicalization_version":"TEST_FIXTURE:v1"`.
Identity agreement is declaration consistency only: unknown algorithms remain
`NOT_RUN`, and authorization remains unverified. No normalizer is executed.
