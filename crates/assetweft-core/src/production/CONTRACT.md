# Production acceptance boundary

`domain` owns Candidate, ComponentSnapshot, ComponentEvidence, AcceptanceReport
and Acceptance with their existing serde shapes. The public candidate paths
explicitly re-export these values; Application signatures/errors are unchanged.
Catalog owns Current and FixtureContract. No external report authorizes acceptance.

The crate-private `application::attempt` use case reads an existing receipt first,
loads the candidate, handles absent contracts/required quality, validates proposed
components and remeasures base bytes before committing. Unsupported reports remain
NOT_RUN, failed structure/roles remain Rejected. Only that controlled path can
construct VerifiedAcceptance (private fields, no deserialization/constructor).
Current-contract/no-current policy is evaluated by that decision against the head
read inside commit. Passing fixture structure never implies quality or production.

Core-owned AcceptanceInputs and CommitAcceptance exchange business values and
stable AcceptanceError categories, never paths, connections, row counts or vendor
errors. The concrete sqlite/acceptance adapter delegates actual bounded validation
to the existing candidate helper; no parsing/provider work occurs in a write txn.
The attempt use case threads the candidate's declared bound into base remeasurement,
which checks the actual managed file before CAS hashing. Its commit takes storage
lease, rechecks both frozen reference lists, checks owner,
opens IMMEDIATE, checks schema, reads duplicate receipt, conditionally advances
Catalog head and writes Production aw_candidate_attempts in one commit. Catalog
owns head SQL; the cross-context adapter translates changed rows to Accepted or
Conflict. Receipt/encoding/insert failures roll back head changes; writes never
replay. String diagnostics and all LibraryError categories round trip at the facade.

Application assembles the adapter and use case directly for attempt, proposal,
staging, initialization, descriptor/receipt queries and controlled validation IO;
`candidate.rs` is only a compatibility value re-export surface. Production owns
only the workflow, staging and attempt tables described below; budget stays in
Consumption, Catalog versions and references stay in Catalog, and schema/migrations/
product acceptance scope are unchanged. Existing real-file candidate integration
tests remain unchanged; port substitutes test orchestration only, not SQLite
correctness.

## Persistent workflows

`workflow_domain` owns the SAME Step/Outcome/State/StepRecord values explicitly
re-exported by `workflow`, graph/bounds validation, persisted state/result policy
and ordinal ready/dependency policy. `workflow_application` owns create/read/
claim/finish/cancel/run use cases through WorkflowQuery/WorkflowRuns. Their error
is an associated stable application category, with PolicyError translated at the
concrete boundary; no DB handle, SQL row count or vendor type enters core.

`workflow_sqlite` is the sole runtime writer of aw_workflow_runs,
aw_workflow_steps and aw_workflow_dependencies. It retains original SQL/parameters,
IMMEDIATE transaction and schema checks. The existing SQL ready projection is
confirmed by pure core policy against records/dependencies read within that SAME
transaction; claim is committed before returning. Cancellation prevents future
claims without changing results. RUNNING blocks all further claims until explicit
finish/reconciliation; outcome recording failure or driver panic does not replay.
Original wire/types/errors and ordinal order remain stable. `workflow` composes
adapters/use cases; Application delegates the actual driver orchestration there.
No automatic invocation, remote execution, paid inference or new functionality.
Existing true-file workflow integration assertions remain unchanged; pure port
checks establish sequencing only. Full current-head CI and independent review are
root-owned.

## Candidate staging ownership

`staging_domain::Proposal` is the SAME value re-exported at `candidate::Proposal`.
Pure staging policy owns identity/revision/fixture-contract/editable validation,
component identity/source checks and exact role/lock descriptor construction.
`staging_application::{propose,read}` are actual use cases; Staging supplies exact
Catalog owner facts and measured snapshots and freezes an internally constructed
FrozenProposal. CandidateRead supplies the read-only query. Neither port exposes
SQLite, paths, CAS handles or trusted acceptance reports. The read use case binds
the persisted descriptor's embedded candidate id to its lookup key and rejects a
mismatch before any acceptance measurement, storage lease or write; the acceptance
commit and both receipt queries therefore reuse that single validated identity.
AcceptanceError's stable
categories are reused internally without changing the accepted acceptance core.

`staging_sqlite` owns descriptor SQL/encoding and bounded managed-byte helpers.
The use cases forward the already-validated declared `max_component_bytes` to the
snapshot port, which rejects the actual managed regular file on that bound before
Catalog managed CAS hashing or Assurance validation; without a predeclared contract
the existing 16 MiB staged ceiling and its diagnostics are retained. Freeze encodes
before storage lease, rechecks ordered references under that lease,
then opens IMMEDIATE, checks current schema and compares the exact stored encoded
value before inserting/committing. Existing triggers retain frozen references;
no write replay, SQL/parameter/check ordering or acceptance authority is added.
Application assembles `staging_sqlite` with the `staging_application` use cases;
no legacy private forward and no new Catalog head API is introduced. Unchanged
candidate integration tests exercise SQLite/CAS behavior; pure substitute tests
prove use-case ordering/policy only.

`specification` owns the pure GenerationSpecRequest/TrustedContext values,
`validate` declaration policy and `recipe_examples`. Legacy `generation_spec`
explicitly re-exports the same items. Shared LockMode/LockStatus remain shared;
this module adds no persistence, port or execution capability. Serde shape,
field-path diagnostics, exact whitespace handling and trusted-lock indexing
are unchanged. Declaration consistency is neither rights confirmation nor
semantic canonicalization execution; INPUT_INCOMPLETE, lock mismatch and
NOT_EXECUTED outcomes retain their existing CLI mappings. Conflicting duplicate
trusted identities (issue 18, P2) remain outside this ownership-only change.

## Fixture initialization and receipt reads

`initialization_application::{initialize,read}` now owns the real bootstrap and
receipt query use cases through Initialization/AttemptRead. It reuses SAME Catalog
Current/FixtureContract and Production snapshot/report/Acceptance values. Policy
retains identity, contract, required-quality, owner source/identity, snapshot and
report-pass order. Only this use case constructs VerifiedInitialization: private
fields, no clone/serde/constructor, borrowed version/contract and owned measured
snapshots, consumed once by the adapter. Public callers cannot submit PASS reports.

The concrete `sqlite/initialization.rs` adapter (explicitly mounted from Production)
uses the existing bounded managed-byte snapshot/validator with the validated
contract bound checked before managed hashing, then holds the shared storage lease, rechecks exact references, opens IMMEDIATE, calls unchanged borrowed
Catalog initialization and commits once. No parsing is moved into the transaction;
current0 idempotency, existing-head errors and exact diagnostics remain unchanged.
AttemptRead retains require_current before the original receipt SELECT/parameters
and deserialize error. Application assembles these use cases with the mounted
`initialization_sqlite` adapter; no legacy forward remains. Pure tests establish
policy/order only; unchanged candidate real-file tests exercise actual SQLite/CAS.

## Scoped Rigged Simple motion staging

The separate `Application::propose_rigged_simple_motion` entry admits just the
pinned original and idle/strike GLB pair. The four managed component versions
bind exact indexed payloads to their owner's bytes. Mesh, materials and rig
must also reuse the base version IDs. The normal freeze transaction rechecks
references and preserves the immutable descriptor. With no declared contract this path retains its original staging-only scope.

## Opt-in Rigged Simple structural Current

An omitted contract retains the PR127 staging scope and its historical NotRun
receipt. A new proposal may predeclare `RIGGED_SIMPLE_MOTION_STRUCTURE_V1`
with a fixed 16 MiB ceiling per managed object and empty required_quality.
Initialization validates the original owner and its four dependencies. Attempt
remeasures both pinned owners and their complete indexed dependency projections,
checks their frozen references and the three unchanged locks, and runs the
Assurance ANIMATED_CHARACTER format and bound-joint role check on both GLBs.
The report's two evidence rows identify the whole owners. The candidate
snapshots identify each of the eight component selections. The existing
Catalog conditional update and Production receipt share one transaction.
Visual quality, playback, engine behavior, scheduling and rights verification
remain unmeasured by this receipt. Schema 6 and the fixture wire contract are preserved.

## Scheduling the pinned motion edit

`Application::schedule_rigged_simple_motion` reads the declared structural head
and revision, then measures the original and its frozen Catalog references.
The idle/strike writer reads the existing tracks and bound mesh/skin topology.
The original's mesh, materials and rig projections have no motion input.
Production intersects those component inputs with the declared edit and creates
the affected steps in the existing workflow. The proposal ID names the run.
The returned base snapshot identifies the inputs used for planning. A concurrent
head advance after planning is adjudicated by the existing acceptance CAS.
Scheduling, claim, driver invocation, outcome recording and structural acceptance
each have separate evidence. The structural receipt retains its historical scope.
