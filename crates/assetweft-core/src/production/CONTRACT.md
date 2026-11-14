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
Its commit takes storage lease, rechecks both frozen reference lists, checks owner,
opens IMMEDIATE, checks schema, reads duplicate receipt, conditionally advances
Catalog head and writes Production aw_candidate_attempts in one commit. Catalog
owns head SQL; the cross-context adapter translates changed rows to Accepted or
Conflict. Receipt/encoding/insert failures roll back head changes; writes never
replay. String diagnostics and all LibraryError categories round trip at the facade.

Compatibility Application assembles the adapter and use case. Proposal, staging,
initialization, descriptor/receipt queries and controlled validation IO remain in
candidate.rs as explicit migration debt. Other Production workflows/budget/history,
Catalog versions and references, schema/migrations and product acceptance scope
are outside this batch. Existing real-file candidate integration tests remain
unchanged; port substitutes test orchestration only, not SQLite correctness.

## Persistent workflows (W2a)

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
checks establish sequencing only. Full current-head CI and independent review
remain root-owned, not established by this local migration.

## Candidate staging ownership (W2b)

`staging_domain::Proposal` is the SAME value re-exported at `candidate::Proposal`.
Pure staging policy owns identity/revision/fixture-contract/editable validation,
component identity/source checks and exact role/lock descriptor construction.
`staging_application::{propose,read}` are actual use cases; Staging supplies exact
Catalog owner facts and measured snapshots and freezes an internally constructed
FrozenProposal. CandidateRead supplies the read-only query. Neither port exposes
SQLite, paths, CAS handles or trusted acceptance reports. AcceptanceError's stable
categories are reused internally without changing the accepted acceptance core.

`staging_sqlite` owns descriptor SQL/encoding and bounded managed-byte helpers.
Freeze encodes before storage lease, rechecks ordered references under that lease,
then opens IMMEDIATE, checks current schema and compares the exact stored encoded
value before inserting/committing. Existing triggers retain frozen references;
no write replay, SQL/parameter/check ordering or acceptance authority is added.
Legacy candidate helpers explicitly forward to this concrete adapter for acceptance.
Initialization/current-head composition and receipt query remain concrete legacy
migration debt; no new Catalog head API is introduced. This section supersedes the
older staging-debt sentence above only for the proposal/read paths migrated here.
Unchanged candidate integration tests exercise SQLite/CAS behavior; pure substitute
tests prove use-case ordering/policy only. Full CI and independent review are root-owned.

`specification` owns the pure GenerationSpecRequest/TrustedContext values,
`validate` declaration policy and `recipe_examples`. Legacy `generation_spec`
explicitly re-exports the same items. Shared LockMode/LockStatus remain shared;
this module adds no persistence, port or execution capability. Serde shape,
field-path diagnostics, exact whitespace handling and trusted-lock indexing
are unchanged. Declaration consistency is neither rights confirmation nor
semantic canonicalization execution; INPUT_INCOMPLETE, lock mismatch and
NOT_EXECUTED outcomes retain their existing CLI mappings. Conflicting duplicate
trusted identities (issue 18, P2) remain outside this ownership-only change.

## Fixture initialization and receipt reads (W2d)

`initialization_application::{initialize,read}` now owns the real bootstrap and
receipt query use cases through Initialization/AttemptRead. It reuses SAME Catalog
Current/FixtureContract and Production snapshot/report/Acceptance values. Policy
retains identity, contract, required-quality, owner source/identity, snapshot and
report-pass order. Only this use case constructs VerifiedInitialization: private
fields, no clone/serde/constructor, borrowed version/contract and owned measured
snapshots, consumed once by the adapter. Public callers cannot submit PASS reports.

The concrete `sqlite/initialization.rs` adapter (explicitly mounted from Production)
uses the existing bounded managed-byte snapshot/validator, then holds the shared
storage lease, rechecks exact references, opens IMMEDIATE, calls unchanged borrowed
Catalog initialization and commits once. No parsing is moved into the transaction;
current0 idempotency, existing-head errors and exact diagnostics remain unchanged.
AttemptRead retains require_current before the original receipt SELECT/parameters
and deserialize error. Legacy candidate is assembly/helper forwarding only for
these paths. This supersedes the preceding initialization/receipt debt statements;
other workflow/specification/composition migrations and whole DDD acceptance remain
separate. Pure tests establish policy/order only; unchanged candidate real-file
tests exercise actual SQLite/CAS. Full CI and independent review remain root-owned.
