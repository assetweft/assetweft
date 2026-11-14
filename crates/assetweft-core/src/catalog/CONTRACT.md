# Catalog AssetHead boundary

Catalog owns `domain::Current` and `domain::FixtureContract`, including their
existing field names, derives and serde representation. The domain depends only
on serde. `candidate::{Current, FixtureContract}` remain compatibility re-exports;
Application's existing methods and acceptance wire representation remain unchanged.

The crate-private concrete SQLite adapter API consists of `current(Connection, asset)`,
`initialize(Transaction, asset, version, contract)` and
`advance(Transaction, asset, base, expected_revision, version)`.
It owns all runtime reads, initial inserts and conditional updates of
`aw_candidate_current`; migration SQL and frozen-reference triggers remain intact.
There is no generic repository or public raw-database API.

The compatibility Application facade currently reads directly through this adapter API. Candidate orchestration validates
fixture source, GLB structure, measured managed bytes, locked references and the
predeclared contract before registration. Initialization is idempotent only for
the same version, revision zero and contract; it cannot reset an existing head.
Advance matches both expected version and revision and checks revision overflow.
The changed-row count remains adapter-internal; the single-database acceptance
adapter translates it into Production Accepted or Conflict. Catalog imports no
Production acceptance types.

The single-database acceptance adapter acquires the storage lease before opening
its Immediate SQLite transaction, then lends that transaction to Catalog. Head
advance and candidate attempt receipt are committed together by that adapter; receipt failure
rolls back the head. The adapter never opens or commits a transaction or waits for
a lease. Duplicate receipts, readonly/unknown-schema errors, UNKNOWN/MISSING
metadata and fixture-only scope retain their existing handling.

This batch covers AssetHead only. Version/object/reference and feedback ownership
still use their existing library/storage/history modules. No production acceptance,
validation capability, schema migration or broader D1 completion is implied.

Hexagonal migration status: these Connection/Transaction functions remain
adapter-internal APIs. The Production acceptance use case depends on its own
semantic CommitAcceptance port; sqlite/acceptance coordinates Catalog CAS and the
Production attempt receipt internally in one transaction. The compatibility facade
still reads this adapter directly, and candidate initialization still borrows its
transaction. Version/reference/staging and initialization separation remain future
batches; this does not claim overall DDD completion.
