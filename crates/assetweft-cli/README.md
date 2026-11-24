# assetweft CLI v1 (AW-033:A/B)

Build with `cargo build -p assetweft-cli --locked`; run `target/debug/assetweft`.
The optional `rig-bend` command runs a local Blender executable. Other commands run without Blender.
`inspect`/`validate`/`plan` never touch a database. Explicit `library init` initializes
a named local library; `workflow create` persists a graph, `workflow status` reads state,
and `workflow cancel` persists a cancellation request. `budget approve` stores an immutable local allowance declaration and `budget balance`
reads its local ledger. `budget reserve` stores a local call intent, `budget intent`
reads it, `budget receipts` reads its stored usage receipts, and `budget cancel` requests local cancellation. These commands never execute
steps or call providers. `candidate show/receipt` query stored local descriptors and
historical acceptance receipts through Production. `candidate initialize` explicitly creates a local fixture head and `candidate propose` stages a managed byte fixture descriptor. `catalog version/references/current`
query stored exact metadata and the local fixture head through Catalog.

```
assetweft [--call-id ID] inspect FILE.glb
assetweft [--call-id ID] rigged-simple-components RiggedSimple.glb --out-dir NEW_DIRECTORY
assetweft [--call-id ID] motion-stance SAMPLES.json --role grounded-humanoid|hovering
assetweft [--call-id ID] motion-strike RiggedSimple.glb [--with-idle] --out NEW_FILE.glb
assetweft [--call-id ID] rig-bend RiggedSimple.glb --blender ABSOLUTE_TRUSTED_EXECUTABLE --out NEW_DIRECTORY
assetweft [--call-id ID] validate FILE.glb [--role static-mesh|rigged-character|animated-character]
assetweft [--call-id ID] plan --request request.json --trusted-context context.json
assetweft [--call-id ID] library init --db PATH
assetweft [--call-id ID] library backup --db EXISTING_DB --out NEW_DIRECTORY
assetweft [--call-id ID] library restore --from BACKUP_DIRECTORY --dest NEW_DIRECTORY
assetweft [--call-id ID] workflow create --db EXISTING_DB --run-id RUN --steps FILE
assetweft [--call-id ID] workflow status --db EXISTING_DB --run-id RUN
assetweft [--call-id ID] workflow cancel --db EXISTING_DB --run-id RUN
assetweft [--call-id ID] budget approve --db EXISTING_DB --allowance FILE
assetweft [--call-id ID] budget balance --db EXISTING_DB --budget-id ID
assetweft [--call-id ID] budget reserve --db EXISTING_DB --request FILE
assetweft [--call-id ID] budget intent --db EXISTING_DB --intent-id ID
assetweft [--call-id ID] budget receipts --db EXISTING_DB --intent-id ID
assetweft [--call-id ID] budget cancel --db EXISTING_DB --intent-id ID
assetweft [--call-id ID] byo-gpu profiles
assetweft [--call-id ID] byo-gpu kaggle-validate --bundle DIR
assetweft [--call-id ID] byo-gpu run --db EXISTING_DB --budget-id ID --intent-id ID --profile PROFILE --model MODEL --out-dir DIR [--image FILE] [--prompt TEXT] [--bundle DIR] [--adapter-bin CMD] [--allow-remote-upload]
assetweft [--call-id ID] candidate initialize --db EXISTING_DB --version-id ID --contract FILE
assetweft [--call-id ID] candidate propose --db EXISTING_DB --proposal FILE
assetweft [--call-id ID] candidate propose-rigged-simple-motion --db EXISTING_DB --proposal FILE
assetweft [--call-id ID] candidate show --db EXISTING_DB --candidate-id ID
assetweft [--call-id ID] candidate receipt --db EXISTING_DB --candidate-id ID
assetweft [--call-id ID] catalog version --db EXISTING_DB --version-id ID
assetweft [--call-id ID] catalog list --db EXISTING_DB [--limit 1..100]
assetweft [--call-id ID] catalog search --db EXISTING_DB --query TEXT
assetweft [--call-id ID] catalog file-availability --db EXISTING_DB --version-id ID
assetweft [--call-id ID] catalog references --db EXISTING_DB --version-id ID
assetweft [--call-id ID] catalog current --db EXISTING_DB --asset-id ID
assetweft [--call-id ID] catalog feedback-add --db EXISTING_DB --feedback-id ID --asset-id ID --version-id ID --decision ACCEPT|REJECT [--reason TEXT]
assetweft [--call-id ID] catalog feedback-latest --db EXISTING_DB --asset-id ID [--version-id ID]
assetweft [--call-id ID] catalog feedback-get --db EXISTING_DB --asset-id ID --version-id ID --feedback-id ID
```

Global `--max-bytes N` bounds each file/JSON input separately (default and hard ceiling
16777216, minimum 1), including `workflow create --steps`, `budget approve --allowance`
and `budget reserve --request`, `candidate initialize --contract` and `candidate propose --proposal`; it never bounds database size or query result sizes and
has no effect on library init/status/cancel/balance/intent/receipts/candidate/catalog queries or feedback-add.
Its existing numeric parser validation still applies to every command.
`library backup` and `library restore` use the core backup limits independently of `--max-bytes`.
The optional [Rigged Simple motion profile](../../examples/rigged-simple/README.md) appends a reproducible, in-place strike to a new GLB. It uses the same bounded input reader.
Paths are OS paths and support spaces/Unicode. File inputs are read without mutation;
`motion-strike`, `rig-bend`, `library init/backup/restore`, `workflow create/cancel`, `budget approve/reserve/cancel` and `candidate initialize/propose` are explicit mutating paths.
All database opens also configure connection pragmas as described below.
Every invocation emits exactly one compact JSON document and newline on stdout:
`schema_version: 1`, `command`, `call_id` (null if absent/unparsed), `status`,
`errors: [{code,field_path,message}]`, `results` (null on input errors).
`command` is `inspect`, `motion-stance`, `motion-strike`, `rig-bend`, `validate`, `plan`, `library init`, `library backup`, `library restore`, `workflow create`,
`workflow status`, `workflow cancel`, `budget approve`, `budget balance`,
`budget reserve`, `budget intent`, `budget receipts`, `budget cancel`, `candidate initialize`, `candidate propose`, `candidate show`, `candidate receipt`,
`catalog version`, `catalog search`, `catalog file-availability`, `catalog references`, `catalog current`, `catalog feedback-add`,
`catalog feedback-latest`, `catalog feedback-get`, `byo-gpu profiles`, `byo-gpu kaggle-validate`, `byo-gpu run`, `arguments`,
`help` or `version`; nested values
are unambiguous command discriminators.
Help/version are JSON results with status HELP/VERSION and exit 0, never plain
stdout. Argument diagnostics also go to stderr. Supply `--call-id` before invalid
arguments when correlating syntax errors: clap cannot recover tokens after the
first unrecognized argument. Valid invocations accept the global ID on either
side of the subcommand; help/version preserve it as well. No timestamps/random IDs.
Repeated calls with identical inputs and identical persisted local state produce
identical stdout. OS I/O diagnostics can vary with environment; they are not a
cross-platform canonical digest.

`inspect` delegates static GLB structure inspection to core geometry. PASS means
only that inspection returned a report; unknown units stay UNKNOWN and the
report lists unexecuted checks. It does not render or read linked URIs.

### Sampled foot slip

`motion-stance` reads one clip's sampled marker tracks. The caller declares the motion
role, support-foot marker and inclusive stance interval. Supply consecutive frames
starting at index 0, in world metres with Y up. A null position means missing evidence.
For example, this programmatic left-sole track slides out 0.125 m and returns:

```json
{"stance":{"contact":"left_sole","frames":[0,2]},"tracks":[{"marker":"left_sole","world_y_up_m":[[0,0,0],[0.125,0,0],[0,0,0]]}]}
```

For `grounded-humanoid`, the check measures the largest horizontal (XZ) distance
from the first stance sample. It rounds the peak to the nearest 1 nm with
`round_ties_even` and returns PASS/0 at or below 0.01 m. The receipt preserves
the raw unrounded `max_slip_m` and reports `limit_m: 0.01`. Binary64 coordinates
lose fine displacement precision at very large world coordinates. Use a nearby
origin when sampling such tracks. The example returns FAIL/1. The same bytes
with `--role hovering` return INAPPLICABLE/5
with a reason and zero evaluated samples. Only PASS returns zero for this command.
Missing, ambiguous or incomplete stance evidence returns NOT_RUN/3. Malformed
track data returns INVALID_INPUT/2. Reports include the sample file SHA-256.

Contact identity, units and role come from the author. The result covers one declared
contact at the supplied frames. Floor height, motion between frames, clip extraction
and character classification are outside this check.

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

## Incoming adapter ownership

`main.rs` owns process I/O, argument grammar/dispatch and explicit library infrastructure
bootstrap. Business adapters translate wire inputs/results through the same existing core APIs:

| Module | CLI responsibility | Core policy owner |
|---|---|---|
| `production.rs` | plan DTO/result projection; workflow create/status/cancel; explicit fixture initialization/staging; stored candidate queries | [Production](../assetweft-core/src/production/CONTRACT.md) |
| `consumption.rs` | strict allowance/request DTOs; budget approve/balance/reserve/intent/cancel | [Consumption](../assetweft-core/src/consumption/CONTRACT.md) |
| `catalog.rs` | declared metadata, exact references, version feedback and fixture-head queries | [Catalog](../assetweft-core/src/catalog/CONTRACT.md) |
| `assurance.rs` | bounded inspect/validate and report projection | [Assurance](../assetweft-core/src/assurance/CONTRACT.md) |
| `execution.rs` | BYO GPU profiles, bundle validation and run receipts | [Execution](../assetweft-core/src/execution/CONTRACT.md) |
| `protocol.rs` | shared envelope/errors, bounded JSON, existing-file admission and source projection | Existing core reader/types/error categories |

Adapters depend inward on these public capabilities and share only transport helpers;
core `Application` retains composition, persistence and business policy. Catalog now has a
real incoming consumer in `catalog.rs`; its legacy `candidate_current` facade is Catalog-owned.

## BYO GPU

`profiles` reports the supported model IDs, licenses, expected artifact names, and
optional external runtimes. `kaggle-validate` checks the bundle on disk and returns
`credential_present` as a boolean. A valid private GPU bundle can pass validation
with the boolean false. Use a real `owner/kernel-slug`, a kernel title, Python
script or notebook, `is_private: true`, `enable_gpu: true`, and an in-bundle code file.
The JSON object accepts boolean flags and the Kaggle CLI 2.2.5 template strings
`"true"` and `"false"`. Duplicate policy fields and numeric kernel ID overrides are
rejected.
Absolute paths, traversal and symlinked code paths are rejected. The accelerator
defaults to `NvidiaTeslaT4` (T4 ×2). A `machine_shape` of `NvidiaL4` is also accepted.

Initialize the library explicitly, then use `budget approve` with `unit: "Task"`,
`approval: "ExplicitBounded"` and positive capacity. The `run` command requires
`--allow-remote-upload` whenever a local image or bundle is supplied. HF 3D models
require an image and HF 2D models require a prompt. Kaggle requires a private
bundle and credential presence for a new attempt. Configure credentials using
`HF_TOKEN`, `KAGGLE_API_TOKEN`, `KAGGLE_USERNAME` with `KAGGLE_KEY`, or a standard
Kaggle credential file such as `~/.kaggle/access_token` or `~/.kaggle/kaggle.json`.
AssetWeft checks file presence without reading the contents.

Set `--adapter-bin` or `ASSETWEFT_BYO_GPU_ADAPTER` to a trusted local executable.
AssetWeft runs it without a shell and with the inherited environment. Use a
runner that keeps credentials in the environment or provider configuration and
sets its own provider deadlines. HF Spaces clients and Kaggle CLI are external
processes. Python is optional for these runners and unnecessary for AssetWeft.
The runner receives `--profile PROFILE --model MODEL --out-dir STAGING_DIR`
plus the supplied inputs as `--image FILE`, `--prompt=TEXT`, or
`--bundle DIR --accelerator ACCELERATOR`. Local file arguments point to private
snapshots. A Kaggle runner should push the staged private kernel with the supplied
accelerator, query that run's status, and download that run's output. PyPI
`kaggle==2.2.5` needs `--with six` when using `uvx`. The runner exits zero only
after writing the expected artifact into its staging output directory.

AssetWeft publishes the artifact under `--out-dir` without replacing an existing
file. Supply an existing directory or a new directory under an existing parent.
The global `--max-bytes` restricts each input file and the output file. On PASS/0,
`results.state` is `SUCCEEDED` and `results.artifact` gives the file name and
SHA-256. Read `budget receipts` for the stored digest. Repeating a settled intent
with the same inputs returns that receipt with `invoked: false` even after
removing the current Kaggle credential. Local staging precedes reservation and
can create a new output directory. A staging failure returns INVALID_INPUT/2 and
leaves the ledger unchanged. An uncertain attempt returns UNKNOWN/1, retains
the reservation, and never launches the runner again for that intent.
`invoked` indicates entry into the claimed runner path, and does not prove a
remote submission. The local unit is `Task` and `cash_cost` stays null.

## Local library and workflow commands

`library init --db PATH` calls `Application::open` then `initialize_library` once.
It may create the named database under an already existing parent and apply the existing
product migrations. It creates no parent directories, chooses no default database, never
downgrades, and does not replace corrupt/unrecognized files on failure. Memory/URI paths,
directories and special files are rejected. Existing files follow core initialization and
migration semantics; this command is an explicit mutation request, so use it only on a
library you intend to initialize/migrate. Repeating init on a current initialized library
preserves workflows, states, outcomes and cancellation requests.

`library backup --db EXISTING_DB --out NEW_DIRECTORY` opens an existing, initialized
SQLite library and writes a consistent snapshot with `library.sqlite`, `objects/` and
`manifest.json`. It may claim the source library's object-store ownership and lease.
`library restore --from BACKUP_DIRECTORY --dest NEW_DIRECTORY` restores a verified
format 1, schema 6 snapshot. Both require a fresh destination and preserve existing
destination contents. A failed copy may leave files in the new directory with no
`manifest.json`. The manifest is published last.

Both return PASS/0 with empty `errors` and the manifest. Backup returns `scope:
backed_up_local_library` and `out_dir`. Restore returns `scope: restored_local_library`
and `dest_dir`. Operation failures return null `results` and an error at `library`.
Missing or nonregular `--db` and database open errors are reported at `db`.
INVALID_INPUT/2 covers those invalid paths, existing destinations, malformed or
unsupported backup manifests, hash mismatches, and backup limits. INPUT_INCOMPLETE/3
covers uninitialized libraries and missing preview sources. A future database schema
reported by the core library returns UNSUPPORTED/5. Storage corruption, backup SQL errors
and cancellation return FAIL/1. Busy and other I/O errors return INTERNAL/6.

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

The following existing-file guard applies to **workflow create/status/cancel, all budget commands, all candidate commands and all Catalog commands**,
not explicit init.

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

## Local immutable budget declarations (AW-033:B bounded slice)

`budget approve --db EXISTING_DB --allowance FILE` reads one JSON object:

```json
{"id":"local-usd-cap","unit":{"CurrencyMinor":{"currency":"USD","decimals":2}},"capacity":12345,"approval":"ExplicitBounded","source":"caller local declaration"}
```

The required fields are exactly `id`, `unit`, `capacity`, `approval`, `source`.
Units use the existing public serde names: `"Token"`, `"Task"`, `"Second"`, or
`{"CurrencyMinor":{"currency":"USD","decimals":2}}`. Approval names are exactly
`"ZeroAdditionalCost"` and `"ExplicitBounded"`. Objects at the allowance and currency
boundaries reject positional arrays, missing/unknown/duplicate fields and invalid types;
malformed JSON, unreadable and oversized input return INVALID_INPUT/2 with field `allowance`.
Numbers are exact unsigned integers: no float, string coercion, truncation, exchange or
conversion. In the example, capacity 12345 is 12345 minor units at decimal scale 2;
no floating-point money is computed. Currency/scale and integer storage bounds are
validated only by Consumption: uppercase three-letter currency, decimals at most 9,
capacity at most 9223372036854775807. The next integer is parsed exactly then rejected by
core. `ZeroAdditionalCost` requires zero capacity; `ExplicitBounded` permits zero capacity.

IDs, currency and source are passed verbatim, with no trim or normalization. Consumption
owns nonempty/bounded/NUL-free declaration text validation. An empty declaration ID is
therefore INVALID_INPUT/2 from core. `budget balance` passes even an empty `--budget-id`
verbatim to the query: an absent identity (including empty) currently returns core Database
FAIL/1, not an invented NotFound or CLI text policy. All facade errors use the existing
variant mapping above, with field `budget`; path/open errors use `db`.

Approve calls `Application::approve_budget` once and returns only
`results: {"scope":"stored_local_allowance_declaration","allowance":{...}}`.
The allowance is an immutable identity: repeating the same typed declaration is a core
no-op success, including JSON key-order changes. A different unit/capacity/approval/source
under the same ID fails and preserves the original declaration, reserved and spent values.
This is no top-up, reset or upsert. There is no extra read or retry after a write, including
an uncertain result or stdout failure. A write may have committed before exit 6; query the
balance to inspect local state rather than automatically replaying the command.

`budget balance --db EXISTING_DB --budget-id ID` calls `Application::budget_balance`
once and projects exactly the returned allowance, reserved and spent integers:

```json
{"schema_version":1,"command":"budget balance","call_id":null,"status":"PASS","errors":[],"results":{"scope":"local_budget_balance","allowance":{"id":"local-usd-cap","unit":{"CurrencyMinor":{"currency":"USD","decimals":2}},"capacity":12345,"approval":"ExplicitBounded","source":"caller local declaration"},"reserved":0,"spent":0}}
```

The query uses existing SELECTs; opens still configure connection pragmas and retain the
existing path precheck race described above. This is not a filesystem read-only guarantee.
There is no derived remaining balance, price, unknown-charge-as-zero or authorization flag.
Zero reserved/spent describes this local ledger only, never an external fee claim.

Allowances, approval names and sources are **caller local declarations**, not authenticated
user consent, paid approval, verified provider pricing, licenses or proof of a free provider
call. No transfer, payment, billing, account creation, network, submit/reconcile/execution
budget command is added. CLI policy stays in the existing Consumption
Application facade; no SQL/connection, core API or schema change is added. This slice does
not complete full AW-033:B, real provider or product acceptance.

## Local call-intent ledger operations (AW-033:B bounded slice)

```text
assetweft [--call-id ID] budget reserve --db EXISTING_DB --request FILE
assetweft [--call-id ID] budget intent --db EXISTING_DB --intent-id ID
assetweft [--call-id ID] budget receipts --db EXISTING_DB --intent-id ID
assetweft [--call-id ID] budget cancel --db EXISTING_DB --intent-id ID
```

These commands use the existing-file guard. `reserve`, `intent`, and `cancel` use
`Application::open` with the inherited check/open race and connection pragmas.
`receipts` uses `Application::open_read_only`. `--call-id` is envelope correlation only, separate from
`request.id` and the verbatim `--intent-id`.

`reserve` reads one bounded, strict JSON **object** through the existing `--max-bytes` reader:

```json
{"id":"intent-1","account":"local-usd-cap","input_fingerprint":"caller-input-v1","unit":{"CurrencyMinor":{"currency":"USD","decimals":2}},"quote":null,"reservation":100,"source":"caller local declaration"}
```

`id`, `account`, `input_fingerprint`, `unit`, `reservation`, and `source` are required.
`quote` may be omitted or null (unknown); integer `0` means the distinct explicit `Some(0)`.
Request and nested currency objects reject positional arrays, unknown/duplicate/missing
required keys, wrong types, floats, negative unsigned values and unsigned overflow.
Wire/file failures return INVALID_INPUT/2 with field `request`. Unit encoding is the same
as allowances. Strings and unsigned integers pass unchanged to Consumption; the CLI does
not normalize text, convert units/currency, fill unknown prices with zero or reproduce
core policy. Consumption alone checks bounded text, currency/scale, i64 storage limits,
quote versus reservation, unit/approval compatibility and remaining capacity. A rejected
request does not add an intent or reservation. Facade errors retain the exact existing
`LibraryError` mapping and message, with field `budget`.

`reserve` calls `Application::reserve_call` once and projects the **returned** intent:

```json
{"scope":"local_call_intent","intent":{"request":{"id":"intent-1","account":"local-usd-cap","input_fingerprint":"caller-input-v1","unit":{"CurrencyMinor":{"currency":"USD","decimals":2}},"quote":null,"reservation":100,"source":"caller local declaration"},"state":"PREPARING","cancelled":false,"charge":null}}
```

The same identity and exact typed descriptor returns its existing durable intent, including
uncertain or terminal states; a conflicting descriptor fails without replacing it. No new
reservation or replay is fabricated. `intent` calls `Application::call_intent` once and
uses the same result shape, including `intent:null` when the API returns None (also for an
absent/empty ID). State is explicitly one of `PREPARING`, `ATTEMPTING`, `UNKNOWN`,
`CONFIRMED`, `RELEASED`; charge is the returned integer or null, never an inferred fee zero.

`receipts` passes the exact `--intent-id` to `Application::call_receipts` once.
PASS/0 returns `results.scope` = `local_call_receipts` and `results.receipts`, an array of
persisted receipts in receipt-ID order. An unknown or empty ID, or an intent with no
receipts, returns an empty array. Each receipt includes `id`, `source`, `resolution`,
and `usage`. Each usage entry preserves its `source`, `unit`, and `amount`.
Units are `"Token"`, `"Task"`, `"Second"`, or
`{"CurrencyMinor":{"currency":"USD","decimals":2}}` (example currency and scale).
`amount: null` is unknown and `amount: 0` is a recorded zero. This command reads
local evidence through SQLite and makes no provider request.

`cancel` calls `Application::cancel_call` once. Its success result is only
`{"scope":"stored_local_call_cancellation_request","intent_id":"intent-1"}`.
There is no extra read after the write, claimed release/refund/stop, or synthesized receipt.
Query `intent` separately for actual durable state. An unknown cancellation ID fails with
the real core INVALID_INPUT/2 mapping. Core releases a never-submitted PREPARING reservation
once without a receipt, retains ATTEMPTING/UNKNOWN reservations, and never refunds
CONFIRMED/RELEASED again. Re-reserving an identical released intent returns RELEASED and
does not renew its reservation. All of this policy stays in Consumption.

PASS/0 is local ledger success only. These are caller declarations, not authentication,
rights, approved provider prices or payment. Cancellation does not promise remote work
or fees stop. No CLI submit, reconcile, receipt fabrication, executor/provider/process
launch, paid/real-AI or production behavior is added. There is no retry after any result or
stdout failure: a mutation can commit before output fails with exit 6; inspect durable
intent/balance separately. This slice is not full AW-033:B or product acceptance.

Exit mapping (CONTRACTS §10.5): 0 scoped checks pass / declaration consistent /
library initialized / workflow created / status read / cancellation request persisted /
local allowance stored, balance/intent/receipts or Catalog metadata/head read, or local intent reserved/cancellation requested /
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


## Stored candidate queries (AW-033:B bounded slice)

Both commands require an existing initialized local library and use the existing-file guard
above. They never initialize/migrate, propose, accept, generate, submit or reconcile. The
metadata precheck and `Application::open` pragmas do not guarantee filesystem read-only
behavior; a disappearance race can still create a file. Business records are only queried.

`--candidate-id` is passed verbatim to core (no trim or normalization). Core alone owns
identity validation, lookup/embedded identity binding, schema and decoding. `--call-id` is
independent correlation data. Errors use the existing LibraryError variant mapping above,
with `field_path: candidate`; path/open failures use `db`. No typed NotFound is invented.

`candidate show` calls `Application::candidate` once. Example complete v1 wire for a valid
missing ID (one compact line plus newline):

```json
{"schema_version":1,"command":"candidate show","call_id":"lookup-1","status":"PASS","errors":[],"results":{"scope":"stored_local_candidate_descriptor","candidate":null}}
```

For a stored row, `candidate` is the exact existing Candidate serde object, including id,
base_version, candidate_version, expected_revision, editable, measured base/proposed
snapshots, source, scope, not_run and contract (including null). It performs no remeasurement,
proposed-component validation or quality assessment.

`candidate receipt` calls `Application::candidate_attempt` once. A valid missing receipt,
including a staged candidate with no attempt, returns:

```json
{"schema_version":1,"command":"candidate receipt","call_id":"lookup-2","status":"PASS","errors":[],"results":{"scope":"stored_local_candidate_acceptance_receipt","receipt":null}}
```

A stored receipt retains the existing externally tagged serde enum: exactly one of
`{"NotRun":{"candidate":...,"reason":...,"report":null}}`,
`{"Rejected":{"candidate":...,"report":...}}`,
`{"Accepted":{"candidate":...,"current":...,"report":...}}`, or
`{"Conflict":{"candidate":...,"current":...,"report":...}}` (ellipsis denotes unchanged
stored values, not literal wire JSON). NotRun can also contain its stored report object.
All nested report scope/source/components/not_run, unknown quality and historical Current
values are preserved. It never fills an absent receipt by attempting acceptance, revalidates
bytes or refreshes Current from Catalog's separate current-head query.

Outer PASS/exit 0 means only the query succeeded; inner NotRun/Rejected/Conflict is unchanged.
A historical Accepted under TEST_FIXTURE_GLB_STRUCTURE_V1 is not fresh validation,
quality/rights/F0/provider/production acceptance or proof that the bytes still exist.
Repeated/reopened queries preserve the stored values even after fixture bytes disappear;
concurrent business writes can change what separate invocations observe. Full AW-033:B,
product acceptance, providers, paid execution, F0, AI and engines remain outside this slice.


## Local version feedback

`catalog feedback-add` requires an existing initialized library and an exact
registered asset/version pair. Decisions are case-sensitive `ACCEPT` or `REJECT`.
The optional reason is passed verbatim. Catalog validates IDs and reasons and
appends one opinion through `Application::append_feedback`. Success returns
PASS/0 with `results.scope: "appended_local_version_feedback"` and a
`results.feedback` record:

```json
{"sequence":1,"feedback_id":"opinion-1","asset_id":"asset","version_id":"v1","decision":"REJECT","reason":"Needs revision"}
```

`catalog feedback-latest` uses `Application::latest_feedback` and returns
PASS/0 with `results.scope: "stored_local_version_feedback"`. It returns the
same record shape, ordered by the persisted local sequence. With `--version-id`
it matches that exact asset/version pair. Without it, it selects the newest
opinion across the asset's versions. No match yields `results.feedback: null`.
An omitted reason is represented as `reason: null` within a record.

`catalog feedback-get` opens the existing library read-only and matches the
exact asset, version and feedback IDs. It returns PASS/0 with the same scope
and record shape as `feedback-latest`. No exact match yields
`results.feedback: null`.

All three commands use regular-file database admission and the v1 JSON envelope.
Duplicate feedback IDs and write-time version/asset mismatches return
INVALID_INPUT/2 at `catalog`. An uninitialized library returns
INPUT_INCOMPLETE/3. Feedback is independent of candidate acceptance and the
current head. A stdout failure can return exit 6 after an append commits.
Use `feedback-get` with the original IDs to confirm that specific append and
check its decision and reason against the attempted write. `feedback-latest`
provides no per-ID confirmation. Never use its result to decide whether to
retry with a new ID after exit 6. `feedback-get` performs no retry.

## Stored Catalog queries (AW-033:B bounded slice)

All three commands use the same `existing_database` regular-file admission and
`Application::open` as the other persisted queries. They do not intentionally create,
initialize or migrate a library: a missing/nonregular path fails INVALID_INPUT/2 at `db`;
a valid empty file remains uninitialized and returns INPUT_INCOMPLETE/3 at `catalog`.
The metadata precheck/open is not atomic, disappearance can race a create-capable open,
and opens configure existing SQLite connection pragmas. No filesystem read-only claim is made.

Each action calls exactly one owning public facade once: `asset_version`,
`component_references`, or `candidate_current`, then translates only its returned value.
IDs pass verbatim without trim, normalization or CLI revalidation. Core owns identity,
schema, decoding and error precedence. In particular, version/reference IDs and asset
head IDs have different core rules; the CLI does not unify them. `--call-id` remains
independent envelope correlation. No retry, extra lookup, latest fallback, expansion,
CAS access, validation, acceptance, head advance or receipt recomputation occurs.
Separate commands do not provide an atomic multi-query snapshot.

`catalog version` returns `results` with exactly
`{"scope":"stored_local_asset_version_metadata","version":VALUE_OR_NULL}`.
The stored object preserves every AssetVersion field: `asset_id`, `version_id`,
`content_hash`, `input_id`, `source`, `authorization`, `authorized`, `license_id`,
`usage_scope`, `source_format`, `contract_version`, and `presence`.
Presence is exactly `UNKNOWN` or `MISSING`; there is no READY state. Authorization,
authorized and license fields are stored declarations, never a new authentication,
rights or availability guarantee. A valid absent identity returns `version:null`.

`catalog list --db EXISTING_DB [--limit 1..100]` reads registered versions in binary asset ID and version ID order. The default limit is 100. The one-line response carries `results.matches` and `results.truncated`. An invalid limit or uninitialized database returns `INPUT_INCOMPLETE` (exit 3).

`catalog search --db EXISTING_DB --query TEXT` opens an existing library read-only.
It matches the case-sensitive literal substring against either the asset ID or the
version ID. It preserves spaces and treats characters such as `%` and `_` literally.
Use `--query='-part'` when the value starts with a dash. An empty query is invalid.

A successful v1 response exits 0 with status `PASS` and `results` containing
`scope:"stored_local_asset_version_search"`, `matches`, and `truncated`.
Each match contains exactly `asset_id`, `version_id`, and `source_format` as stored.
Format is a metadata declaration. Matches use SQLite BINARY order by asset ID,
then version ID, with one row per matching registered version. No match yields
`matches:[]` and `truncated:false`. When more than 100 versions match, only the
first 100 are returned and `truncated:true`. Refine the substring to narrow the
results. This command has no continuation token.
Pass a returned `version_id` as `--version-id` to `catalog version` or
`catalog file-availability`.

`catalog file-availability --db EXISTING_DB --version-id ID` opens SQLite in
read-only mode. A successful query exits 0 with outer status `PASS` and returns
`{"scope":"local_managed_version_bytes","version_id":"ID","availability":"STATE"}`.

| State | Meaning |
|---|---|
| `UNKNOWN_VERSION` | The exact version ID has no local Catalog record. |
| `METADATA_ONLY` | The version has a record without a managed-object binding, even if another version shares its content hash. |
| `MISSING` | The version has a managed-object binding and valid store ownership, but the object file or directory is absent. |
| `AVAILABLE` | The local managed file was readable and matched the bound byte length and SHA-256 during this call. |

Missing or unreadable databases fail at `db` with `INVALID_INPUT`/2. An
uninitialized database returns `INPUT_INCOMPLETE`/3. Corrupt bytes fail with
`FAIL`/1, invalid storage entries or ownership with the existing typed error,
and storage I/O errors such as denied access or a missing owner file with
`INTERNAL`/6. Errors retain a diagnostic and return `results:null`. The query
does not create a library, claim store ownership, or repair missing files.

The observation applies to one version in a private, cooperating local library.
It streams the managed file and is independent of the stored `presence` and
`--max-bytes`. It provides no guarantee about later reads, external references,
preview rendering, format validity, search, or usage rights.

`catalog references` returns
`{"scope":"stored_local_exact_component_references","references":[...]}`.
Every entry preserves `owner_version_id`, `role`, `component_version_id` and the exact
returned order (currently core orders by role then component version). The seven role
strings are `mesh`, `materials`, `rig`, `motion`, `vfx`, `audio`, `dialogue`.
There is no CLI sorting or role parsing. Both an absent owner version and an existing
version with no references retain the returned empty `[]`, without an invented NotFound.

`catalog current` returns
`{"scope":"stored_local_candidate_current","current":VALUE_OR_NULL}`.
It serializes the existing Current unchanged: `asset_id`, `version_id`, `revision`,
`contract`; FixtureContract retains `contract_id`, `max_component_bytes`, `required_quality`.
A valid absent asset returns `current:null`. This is the stored fixture head, distinct
from an historical Accepted receipt and never availability, quality, rights, provider
or product acceptance. No fresh contract validation is performed.

Outer PASS/exit 0 means query success, including null/empty results and metadata-only
UNKNOWN/MISSING. Facade failures use `field_path: catalog` and the existing exhaustive
LibraryError mapping: InvalidInput/Constraint → INVALID_INPUT/2; Uninitialized →
INPUT_INCOMPLETE/3; FutureSchema → UNSUPPORTED/5; Busy/Io → INTERNAL/6;
Database/UnknownValue → FAIL/1. Path/open failures retain INVALID_INPUT/2 at `db`,
including an open-time future-schema failure; no error-string classification is added.
The shared process retains best-effort stderr and stdout-failure exit 6, including
invalid grammar. Global max-bytes parsing is retained but bounds neither DB nor results.

Synthetic actual-binary tests cover both presences/all metadata fields, all seven roles,
ordered exact references/null/empty, real persisted head revisions/contracts, repeat/reopen,
owned fixture CAS disappearance, ID/DB/grammar/help/correlation and broken streams.
Direct old/future/corrupt-schema runtime fixtures are NOT_RUN because public fixture APIs
do not construct them; no raw SQL or corruption is added to fill that coverage.
Full workspace C1, current-SHA CI, native Windows/macOS matrix, different-family independent
review, Git publication/merge, full AW-033:B, product/AI/provider/paid/F0/engine/production
acceptance remain NOT_RUN in this author slice. Root owns those separate gates.


## Explicit local fixture initialization and staging (AW-033:B bounded slice)

`candidate initialize --db EXISTING_DB --version-id ID --contract FILE` reads a strict JSON object:

```json
{"contract_id":"TEST_FIXTURE_GLB_STRUCTURE_V1","max_component_bytes":1048576,"required_quality":[]}
```

All three fields are required. The CLI projects exactly `FixtureContract` and passes the
version ID verbatim to `Application::initialize_candidate_current` once. Production owns
identity, contract limits, fixture source, roles, actual bounded managed bytes, structure,
required quality and existing-head policy. Success is exactly
`results: {"scope":"initialized_local_fixture_current","current":<returned Current>}`.
The returned Current includes asset_id, version_id, revision and the exact declared contract;
an absent fixture head starts at revision 0. Repeating an identical initialization follows
core idempotency; a different existing head/contract is not overwritten. This operation
performs the existing fixture checks, never quality, rights or product acceptance. It does
not initialize/migrate the library, write arbitrary heads, query current again or retry.

`candidate propose --db EXISTING_DB --proposal FILE` reads a strict JSON object:

```json
{"id":"candidate-1","base_version":"base","candidate_version":"proposed","expected_revision":0,"editable":["mesh"],"lock_mode":"BYTE","contract":{"contract_id":"TEST_FIXTURE_GLB_STRUCTURE_V1","max_component_bytes":1048576,"required_quality":[]}}
```

The six fields preceding `contract` are required; `contract` may be omitted or null (None),
or must contain the same strict three-field object above. Identity fields are strings,
expected_revision is an exact signed 64-bit integer, max_component_bytes is an exact
unsigned 64-bit integer, required_quality is an array of strings and editable is an array
of the exact lowercase strings mesh/materials/rig/motion/vfx/audio/dialogue. lock_mode uses
the existing exact strings BYTE or SEMANTIC. The transport does no normalization or
coercion; Production currently rejects SEMANTIC staging, invalid identities/revisions,
role counts/duplicates, contract limits, sources and changed locked components through
its existing typed errors. No semantic policy is recreated in the CLI.

The CLI calls `Application::propose_candidate` once and returns exactly
`results: {"scope":"staged_local_byte_fixture_candidate","candidate":<returned Candidate>}`.
PASS/0 means only that the managed local byte fixture descriptor was staged. Its existing
source/scope/contract/not_run and measured component snapshots are preserved. It does not
advance current, create an acceptance receipt, validate quality or rights, or submit a
caller-provided PASS as authority. An identical typed proposal follows core idempotency;
a conflicting descriptor under the same identity fails without replacing it. Use existing
`candidate show`, `candidate receipt` and `catalog current` to observe durable state.
Candidate attempt is the bounded transport described below; no execute/submit command is exposed.

Both top-level and nested objects reject positional arrays, unknown/duplicate/missing
fields and wrong types, including object-shaped enum tags. Malformed/trailing JSON,
floats or strings in integer fields, out-of-range integers, unreadable/nonregular inputs
and oversized files fail INVALID_INPUT/2. Each input uses the existing bounded reader and
`--max-bytes` (1..16777216); paths are OS paths with Unicode/spaces and the existing symlink
policy. Input files are not modified. Precedence is grammar, existing regular database
admission, Application::open, bounded DTO parsing, then the one owning facade operation.
Path/open errors use field_path db; JSON/file errors use contract or proposal; facade
errors use candidate with the unchanged exhaustive LibraryError mapping above. An existing
empty library therefore returns INPUT_INCOMPLETE/3 on a valid request; malformed DTOs
perform no business writes. Opens still configure pragmas and retain the check/open race;
there is no filesystem read-only or combined atomic multi-read guarantee.

The v1 single JSON line and newline, best-effort stderr and fatal stdout exit 6 apply to
both operations, grammar and errors. An explicit write may already be committed when
stdout fails: exit 6 does not imply rollback and triggers no automatic retry. Inspect the
stored descriptor/head through existing query commands. `--call-id` is correlation only,
not an idempotency token. Fixtures and programmatic GLB checks are synthetic evidence,
not real generated assets, authorization, budget approval, provider access or production
acceptance. Full AW-033:B, product, real-data, paid/provider, F0, AI and engines are NOT_RUN.


## Stage a Rigged Simple motion edit

`candidate propose-rigged-simple-motion` takes the proposal JSON shown above with
`editable: ["motion"]`, `lock_mode: "BYTE"` and an omitted or null contract.
It measures two managed GLB versions of one asset: the original Cesium Rigged
Simple and the pinned derivative with the added strike and idle clips. Both
owners declare CC-BY-4.0 and the derivative declares a procedural edit.

Use `assetweft rigged-simple-components FILE --out-dir NEW_DIRECTORY` to project a
pinned `RiggedSimple.glb` or its verified `motion-strike --with-idle` derivative into
`mesh.json`, `materials.json`, `rig.json`, and `motion.json`. The command uses the same
bounded regular-file admission as validation, creates a new output directory only, and
returns `NOT_RUN/1` without creating it for every other GLB. Its PASS receipt is the
component manifest; the four files are the exact bytes consumed by the candidate loop.
on its GLB bytes. Commit the four returned JSON payloads as managed Catalog
versions and attach them with exact role references. The derivative reuses the
original mesh, materials and rig version IDs and attaches its own motion version.
Keep Cesium's attribution with these derivatives. Version and rights fields
record the caller's declarations.

A successful command freezes the measured descriptor under
`RIGGED_SIMPLE_MOTION_BYTE_STAGING_V1`. Read it after reopening with
`candidate show`, and retrieve individual selected objects with
`catalog materialize-export`. Staging creates no current head. The fixture
`candidate attempt` records `NotRun` for the absent contract. Read that
historical result with `candidate receipt`.

### Advance a local Rigged Simple head

Use the following contract JSON with `candidate initialize --db DB --version-id ORIGINAL --contract FILE`.
Include the same object in the proposal's `contract` field and use a fresh candidate ID.

```json
{"contract_id":"RIGGED_SIMPLE_MOTION_STRUCTURE_V1","max_component_bytes":16777216,"required_quality":[]}
```

To generate the derivative from an initialized original, first use
`candidate schedule-rigged-simple-motion --db DB --proposal FILE`. Supply the same
structural BYTE proposal used for staging. Its ID becomes the workflow run ID.
The response includes the measured base component versions and hashes. Read
`workflow status`, then `workflow claim`. For the claimed motion step, materialize
the exact base version and run `motion-strike SOURCE --with-idle --out NEW_FILE`.
Record the driver's actual outcome with `workflow finish`. An unresolved claim
blocks further claims after restart. Register the generated GLB and motion bytes
and reuse the three locked component versions as references before staging.

Stage with `candidate propose-rigged-simple-motion`, then run
`candidate attempt --db DB --candidate-id ID`. The durable receipt identifies
the two structurally validated GLBs, their managed mesh/material/rig/motion
references, and the local head. A stale expected version or revision yields
Conflict and preserves both candidates. Retry returns the original receipt.
GLB structure and exact locked dependencies are the extent of this decision.
The structural receipt records visual quality, Blender playback, engine import and
workflow scheduling as NOT_RUN. The workflow records the separately claimed step.

## Local fixture candidate acceptance attempt (AW-033:B bounded slice)

`candidate attempt --db EXISTING_DB --candidate-id ID` passes the ID verbatim to
`Application::attempt_accept_candidate` exactly once. It uses the shared existing regular
database admission, never creates/initializes a missing database, and accepts no report or
PASS input. Production owns validation, policy, VerifiedAcceptance, conditional Catalog
head advance and the atomic durable receipt. The CLI makes no extra candidate/current/receipt
queries and performs no retry or fresh validation of its own. Every successful facade return
has exactly `results: {"scope":"local_fixture_candidate_acceptance_attempt","receipt":<returned Acceptance>}`;
the receipt is the unchanged existing core serde value, including candidate, reason, report
and historical Current where present. The exhaustive outer transport projection is:

| Core variant | Status / exit | Error code |
|---|---|---|
| Accepted | PASS / 0 | none (`errors: []`) |
| Rejected | FAIL / 1 | REJECTED |
| Conflict | FAIL / 1 | CONFLICT |
| NotRun | UNSUPPORTED / 5 | NOT_RUN |

Conflict is an expected-head/revision precondition conflict, distinct from a report failure.
Code 5 deliberately coarsely projects incomplete local acceptance capability: missing
predeclared contract, absent current, current-contract mismatch, required quality or
unsupported roles retain the exact core reason/report in the receipt. The adapter does not
parse reason strings or inspect report statuses/counts to invent finer taxonomy. NOT_RUN
is never PASS or FAIL. Facade errors keep the existing exhaustive LibraryError mapping
and null results. Grammar and global `--call-id` / `--max-bytes` remain unchanged; max-bytes
parsing bounds neither the database nor result and does not override the stored contract.

The core persists all four outcomes atomically; only Accepted advances current. Its stored
receipt wins before any new validation. Repeating an attempt returns that original historical
receipt, even after a later head advance or loss of owned CAS bytes; this is core receipt-first
behavior, not CLI retry/replay logic. `--call-id` is correlation only, not idempotency authority.
Use existing query commands for explicit observation. Application::open remains create-capable
with its current pragmas; regular-file admission retains the documented check/open race.

The single v1 JSON line plus newline, best-effort stderr and fatal stdout exit 6 apply to
every outcome, facade error and grammar error. A receipt and head change may have committed
before stdout fails, including when both streams fail. Exit 6 implies neither rollback nor a
safe retry; no automatic retry occurs. Nonaccepted outcomes retain a durable receipt without
advancing the head. PASS for Accepted establishes only the returned declared local fixture
structure, never quality, license/rights, provider authorization, F0, engine, product or
production approval. No generation, provider or payment operation is invoked.

Actual-binary regressions use independent public Application/SQLite/CAS control fixtures,
first-time receipts, all four variants (including missing contract, quality and contract
mismatch), later-head/CAS-loss historical replay, ID/DB/grammar/correlation and native broken
streams. Direct old/future/corrupt-schema fixtures remain NOT_RUN because public APIs do not
construct them. Author evidence is scoped to the tested environment; current full CI, native
Windows matrix, independent review and protected integration are root-owned gates. Full
AW-033:B, real data, AI, engines, provider, paid, F0, product and production remain NOT_RUN.


## Catalog metadata and exact-reference writes (AW-033:B bounded slice)

```sh
assetweft catalog register-version --db EXISTING_DB --version version.json
assetweft catalog add-reference --db EXISTING_DB --reference reference.json
```

`register-version` requires one strict JSON object with all twelve fields:

```json
{"asset_id":"asset-1","version_id":"v1","content_hash":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","input_id":"input-1","source":"DECLARED","authorization":"UNVERIFIED","authorized":false,"license_id":"UNKNOWN","usage_scope":"TEST_ONLY","source_format":"GLB","contract_version":"1.0.0","presence":"UNKNOWN"}
```

Every field is a string except the strict boolean `authorized`. `presence` is exactly
`"UNKNOWN"` or `"MISSING"`. `source_format` remains a free string. Values pass verbatim:
CLI does not trim identities, normalize declarations or decide contract-version policy.
Source, authorization, authorized and license fields are caller declarations; registering
metadata verifies neither rights/consent nor managed bytes, availability or acceptance.

`add-reference` requires exactly three string fields:

```json
{"owner_version_id":"v1","role":"mesh","component_version_id":"mesh-v1"}
```

`role` is exactly one of `mesh`, `materials`, `rig`, `motion`, `vfx`, `audio`, `dialogue`.
No aliases, defaults or latest-version resolution are performed. Core owns identities,
foreign keys, uniqueness, immutable metadata and frozen-reference rules.

Both inputs reject arrays (including positional arrays), missing/extra/duplicate fields,
wrong types and trailing JSON as INVALID_INPUT/2 before invoking a writer. The existing
bounded reader applies global `--max-bytes` (1 through 16777216 inclusive, default
16777216); input paths use OS path values. Admission order is clap grammar, existing
regular database file check, `Application::open`, bounded strict JSON, then exactly one
owning facade call: `add_asset_version` or `add_component_reference`. The precheck is
not race-proof; the later open remains create-capable and may apply connection pragmas.
Neither command initializes or migrates schema. A malformed input can precede the
uninitialized-schema error because parsing precedes the facade. Within Core, version
semantic validation precedes its lease; reference lease acquisition precedes reference
semantic validation. Unchanged LibraryError categories and messages determine failures,
including a future contract-version string; CLI does not parse reason strings.

Success is outer PASS/0 with `errors: []` and exactly one submitted typed projection:

- `{"scope":"registered_local_asset_version_metadata","version":{...}}`
- `{"scope":"registered_local_exact_component_reference","reference":{...}}`

These are write acknowledgements after successful unit-returning Core calls, not fresh
queries or proof of bytes, rights, readiness or Production head advancement. Existing
`catalog version/references/current` commands continue to query stored values. There is
no handler readback, retry or idempotency token: duplicates retain actual Core uniqueness
failures, and `--call-id` is correlation only. The compact v1 JSON plus newline protocol
is unchanged. Stderr is best effort; stdout failure exits 6 even when the mutation has
already committed. That exit does not imply rollback or make automatic replay safe.


`assetweft [--call-id ID] [--max-bytes N] catalog commit-object --db EXISTING_DB --version VERSION_JSON --file SOURCE_FILE --expected-len N`
streams a regular local file through the existing Catalog managed-object commit. Version JSON uses the same strict 12-field metadata object as register-version. Expected length is an explicit u64 (including zero); global max-bytes (1..=16777216) bounds both JSON transport and Core managed bytes. No prior register-version is needed. The database must already be initialized; no implicit initialization/migration occurs.

Success scope `committed_local_managed_object` returns exact version_id and actual Core content_hash/byte_length in one compact v1 JSON line. Core validates metadata/length/hash/cap, publishes before its short registration transaction, and may leave an orphan on later failure. No readback, replay, stale-lock break or partial-state repair is attempted. This verifies local managed bytes only; declarations remain independent of license, quality, readiness, provider or product verification.

Source admission errors are INVALID_INPUT (2). Core Library errors retain the existing mapping; Storage Invalid maps to INVALID_INPUT (2), Corrupt to FAIL (1), Busy/Io to INTERNAL (6). A stdout failure returns 6 even after a durable commit; inspect the existing public state before deciding what to do, never automatically replay. Stderr is best effort. Path prechecks and open have replacement races: this is for private local files, with no adversarial filesystem or filesystem-readonly guarantee. No URL, network, shell or stdin source is accepted.

`workflow claim --db EXISTING_DB --run-id RUN` durably claims at most one ready
step, returning `persisted_workflow_step_claim` with the exact `run_id` and
`step_id` (or null). Null does not classify completion or cancellation. Claim
executes nothing and never auto-finishes.

`workflow finish --db EXISTING_DB --run-id RUN --step-id STEP --outcome FILE`
records a caller-supplied actual outcome for an already RUNNING step. FILE must
be an object with required string `state` (exactly `succeeded` or `failed`) and
string `raw`; optional `evidence` is a string or null. Omission/null means no
pointer; an empty string remains an empty pointer. Unknown/duplicate fields and
positional arrays are rejected. Core enforces UTF-8 byte limits (raw 1048576,
evidence 4096); the global JSON input cap also applies. Successful recording
returns PASS/exit 0 even for a failed outcome. The submitted outcome echo is an
ACK marked `caller_supplied_not_executed_or_verified`, not a fresh database
query or verification of evidence, model output, rights or quality.

After restart, read `workflow status`, explicitly finish only known actual
outcomes of RUNNING work, then claim the next step. Unknown work must never be
replayed. There is no automatic resume, execution, retry, reset or provider call.
Cancellation blocks future claims, preserves results and still permits explicit
finish of RUNNING work; it does not stop remote processes or refund fees.
A stdout failure (exit 6) can follow a committed claim or finish: never retry an
ambiguous mutation automatically. Use status to reconcile it.

`assetweft catalog preview --db EXISTING_DB --version-id ID` generates a
thumbnail or audition clip for one managed PNG, GLB or WAV version and returns its library-relative
path under `previews/`. It verifies the exact source length and SHA-256 on every
call. Static 8-bit RGBA input is limited to 16 MiB, 4096 pixels per axis and
4,194,304 pixels total. Nearest-neighbour sampling preserves RGBA channels and
fits the image inside 128 × 128 without upscaling. An identical preview can be
reused. An unknown version returns `INVALID_INPUT`/2. If a registered managed
version's source file is missing from the validated object directory, preview returns
`INPUT_INCOMPLETE`/3 with `managed source file is missing`. Both errors return
`results:null` and leave existing previews intact. A conflicting file or
unsupported input returns an error.

For WAV, use a canonical RIFF/WAVE with exactly a 16-byte PCM format chunk
and a data chunk, 16-bit mono or stereo at 8,000–48,000 Hz, at least two complete
frames and at most 16 MiB. The result has scope `local_version_wav_preview`
and a `.v1.wav` path. The clip contains the first half of the frames, rounded
down and capped at one second, at the original sample rate. The CLI writes
the playable file without opening an audio device.

For GLB, a single static indexed TRIANGLES primitive with an opaque double-sided
base color can produce a 128 × 128 front-view PNG. It applies node rotation and
scale, fits the transformed geometry with equal margins on the longest axis,
and returns `.glb-v1.png` under the PNG output scope. Inputs are capped at
64 KiB and 256 triangles. See the [Catalog contract](../assetweft-core/src/catalog/CONTRACT.md#exact-version-png-wav-and-glb-previews)
for the supported attributes, material and scene layout.


### Rigged Simple bend measurement

Use the unchanged **Rigged Simple** GLB (© 2017 Cesium) from the [pinned Khronos sample directory](https://github.com/KhronosGroup/glTF-Sample-Assets/tree/f0920f5b32d172a30efe3f9257e965918831e6ea/Models/RiggedSimple).
The [per-model license](https://github.com/KhronosGroup/glTF-Sample-Assets/blob/f0920f5b32d172a30efe3f9257e965918831e6ea/Models/RiggedSimple/LICENSE.md) is [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/).
The GLB SHA-256 is `3a79dabb67bb0cd598a18d08b954d9d357c27c30672f82ef5d3f4e7fe6ca3401`.
The executable must be a trusted local Blender 5.1.2 build `ec6e62d40fa9`.
The output directory must be new. It holds a staged copy of the input, native logs,
and the coordinates measured at rest, after a 90° world +Z rotation of Bone.001, and
with the armature modifier disabled during that rotation. Coordinates are glTF world metres.
The numeric rig check runs before import. The measurement checks all 160 no-op positions
against rest and three reference landmarks against the expected bend, with a
maximum error of 0.00001 m per coordinate. These measurements describe the preset
pose on this sample. The source file and its animation are preserved.
