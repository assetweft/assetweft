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
