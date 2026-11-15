# assetweft CLI v1 (AW-033:A/B)

Build with `cargo build -p assetweft-cli --locked`; run `target/debug/assetweft`.
No daemon, Python, model, shell, external resource resolution, network or remote execution.
`inspect`/`validate`/`plan` never touch a database. Explicit `library init` initializes
a named local library; `workflow create` persists a graph, `workflow status` reads state,
and `workflow cancel` persists a cancellation request. These commands never execute steps.

```
assetweft [--call-id ID] inspect FILE.glb
assetweft [--call-id ID] validate FILE.glb [--role static-mesh|rigged-character|animated-character]
assetweft [--call-id ID] plan --request request.json --trusted-context context.json
assetweft [--call-id ID] library init --db PATH
assetweft [--call-id ID] workflow create --db EXISTING_DB --run-id RUN --steps FILE
assetweft [--call-id ID] workflow status --db EXISTING_DB --run-id RUN
assetweft [--call-id ID] workflow cancel --db EXISTING_DB --run-id RUN
```

Global `--max-bytes N` bounds each file/JSON input separately (default and hard ceiling
16777216, minimum 1), including `workflow create --steps`; it never bounds database size
and has no effect on init/status/cancel.
Paths are OS paths and support spaces/Unicode. File inputs are read without mutation;
`library init`, `workflow create` and `workflow cancel` are explicit mutating paths.
All database opens also configure connection pragmas as described below.
Every invocation emits exactly one compact JSON document and newline on stdout:
`schema_version: 1`, `command`, `call_id` (null if absent/unparsed), `status`,
`errors: [{code,field_path,message}]`, `results` (null on input errors).
`command` is `inspect`, `validate`, `plan`, `library init`, `workflow create`,
`workflow status`, `workflow cancel`, `arguments`, `help` or `version`; nested values
are unambiguous command discriminators.
Help/version are JSON results with status HELP/VERSION and exit 0, never plain
stdout. Argument diagnostics also go to stderr. Supply `--call-id` before invalid
arguments when correlating syntax errors: clap cannot recover tokens after the
first unrecognized argument. Valid invocations accept the global ID on either
side of the subcommand; help/version preserve it as well. No timestamps/random IDs.
Repeated calls with identical inputs and identical persisted workflow state produce
identical stdout. OS I/O diagnostics can vary with environment; they are not a
cross-platform canonical digest.

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

## Explicit library initialization and workflow commands (AW-033:B bounded slice)

`library init --db PATH` calls `Application::open` then `initialize_library` once.
It may create the named database under an already existing parent and apply the existing
product migrations. It creates no parent directories, chooses no default database, never
downgrades, and does not replace corrupt/unrecognized files on failure. Memory/URI paths,
directories and special files are rejected. Existing files follow core initialization and
migration semantics; this command is an explicit mutation request, so use it only on a
library you intend to initialize/migrate. Repeating init on a current initialized library
preserves workflows, states, outcomes and cancellation requests. No backup/migration
framework is added.

`workflow create --db EXISTING_DB --run-id RUN --steps FILE` requires a JSON array:

```json
[
  {"id":"prepare","dependencies":[]},
  {"id":"build","dependencies":["prepare"]}
]
```

Both fields are required on each object (positional arrays are rejected); unknown fields,
invalid types, duplicate fields, malformed JSON, unreadable or
oversized input fail at the wire boundary with INVALID_INPUT/2 (`field_path: steps`).
The existing bounded reader and global `--max-bytes` apply to this file. Parsed identity
strings and array/dependency order are passed unchanged to existing `workflow::Step`
values: no trim, normalization, topological sorting or inferred dependencies. Production
alone validates run/step bounds, uniqueness, dependency existence and cycles, then atomically
inserts the workflow. Graph policy currently returns `LibraryError::Database`, hence FAIL/1;
the CLI does not invent InvalidGraph/NotFound by parsing diagnostics. Duplicate runs fail
with the actual core constraint mapping (INVALID_INPUT/2), preserving original records;
there is no upsert, replay or implicit retry. PASS/0 returns only
`results: {"scope":"persisted_workflow_created","run_id":RUN}`: creation/persistence,
no claim, execution, generation, provider call or budget approval. There is no extra status
read or retry after the write. New steps are pending without outcomes; use status for
current state, which can change through existing product use cases.

The following existing-file guard applies to **workflow create/status/cancel**, not explicit init.

`--db` must name an existing regular file (symlinks followed), checked with filesystem
metadata before the create-capable `Application::open`. Missing/non-regular paths exit 2
and are never created; there is no automatic `initialize_library`, migration, fallback
or default database. An existing but uninitialized file reports INPUT_INCOMPLETE/3 and is
left byte-empty. Old schemas fail through the core diagnostic; future schemas are rejected
at open with exit 2 and the preserved core message because `Application::open` collapses
them into `ProtoError::Other`, so the CLI does not fabricate a typed UNSUPPORTED result.
This precheck is not race-proof: `Application::open` may create the file if it disappears
between check and open, and it configures the SQLite connection (pragmas). This is not a
filesystem read-only guarantee. Current schema 6 is enforced by existing core.

`--run-id` is passed verbatim to Production, never trimmed, renamed, normalized or
defaulted. For status/cancel an empty value exits 2 at the existing boundary; create leaves
all run-identity policy to Production (including empty/oversized/control identities, FAIL/1).
An unknown run fails truthfully (FAIL/1 with the
preserved core `unknown run` message; no typed NotFound is invented). `LibraryError`
mapping is deterministic by variant: `InvalidInput`/`Constraint` 2 INVALID_INPUT;
`Uninitialized` 3 INPUT_INCOMPLETE; `FutureSchema` 5 UNSUPPORTED; `Busy`/`Io` 6 INTERNAL;
`Database`/`UnknownValue` 1 FAIL. Messages are preserved; `field_path` is `db` at the
path/open boundary and `workflow` for persisted-state errors.

`status` returns `run_id`, `cancellation_requested` and ordered `steps` with the exact
persisted `state` (`pending|running|succeeded|failed`), `raw` and `evidence` (`null` for
pending/running) projected from the existing Production records. It reads steps and then
cancellation sequentially; `results.snapshot` is `SEQUENTIAL_STEPS_THEN_CANCELLATION_NOT_ATOMIC`.
Values are exact per query but the pair is not a combined snapshot, and concurrent writers
can interleave. Same persisted state gives same stdout; that is not a concurrency guarantee.
`cancellation_requested` is a durable request only, never proof that work stopped.

`cancel` persists the cancellation request (`scope: persisted_cancellation_request_only`,
`cancellation_requested: true`). There is no retry or replay after an uncertain write. It
never claims a process stopped, remote billing stopped, a refund or budget release; existing
step states/outcomes are unchanged and the existing core policy blocks future claims.
Repeating `cancel` remains a scoped success with identical output.

There is no run/submit/retry/resume/reconcile command; such syntax exits 2.
Init/create/status/cancel never execute work. Production alone owns workflow state; the CLI only parses,
projects and maps errors. This is not full AW-033:B or product acceptance.

Exit mapping (CONTRACTS §10.5): 0 scoped checks pass / declaration consistent /
library initialized / workflow created / status read / cancellation request persisted /
help/version; 1 core validation
or other contract FAIL (including persisted workflow Database diagnostics); 2 malformed
arguments, unreadable/oversized files, malformed JSON/GLB inspection, invalid or non-regular
`--db`, empty status/cancel `--run-id`, or core INVALID_INPUT; 3 core INPUT_INCOMPLETE (including an
uninitialized library); 4 core LOCK_VIOLATION; 5 core UNSUPPORTED; 6 output I/O or
unexpected internal failure. Core semantic missing fields use code 3; absent CLI syntax
uses 2. Multiple plan issue categories use priority 4,2,3,5,1; all issues remain visible.
Unimplemented execution does not become a failure or a successful execution.

Dependency decision (2026-11-12): exact clap 4.6.7, official
https://docs.rs/clap/4.6.7/clap/struct.Command.html and crates.io sparse index
https://index.crates.io/cl/ap/clap plus the published package Cargo.toml confirm
version, MIT OR Apache-2.0, MSRV 1.85 (workspace 1.97). Builder API features only:
std/help/usage/error-context; no derive, color, suggestions or platform runtime.
serde/serde_json reuse exact existing pins. No second schema engine. Geometry's
Serialize derives expose its existing report tree directly to this consumer;
no geometry or validation behavior changed. Black-box tests exercise that tree.
Workflow DTOs are CLI-local projections over the non-Serialize core records; no new
core API, SQL, connection or business policy is added. The existing `Application`
use cases own initialization, workflow creation, reads and cancellation writes.

C2 is `cargo test -p assetweft-cli --locked` within existing workspace C1.
Synthetic fixtures demonstrate local behavior only. Direct old/future-schema CLI fixtures
are NOT_RUN where setup would require forbidden SQL/dependencies; existing core guards
are retained. Windows/F0/AI/payment/production
remain NOT_RUN. Current-SHA CI and independent non-author review are separate root gates.

SEMANTIC lock declarations in both `locked_components` and independent
`declared_locks` require `canonicalization_id` and `canonicalization_version`
(nonblank opaque strings, compared verbatim). BYTE does not require them.
Old SEMANTIC declarations without these fields now intentionally return
`INPUT_INCOMPLETE` (exit 3); differing complete identities return
`LOCK_VIOLATION` (exit 4). Example identity fields:
`"canonicalization_id":"TEST_FIXTURE:rig","canonicalization_version":"TEST_FIXTURE:v1"`.
Identity agreement is declaration consistency only: unknown algorithms remain
`NOT_RUN`, and authorization remains unverified. No normalizer is executed.
