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
The returned changed-row count distinguishes Accepted from Conflict.

Candidate orchestration acquires the storage lease before opening its Immediate
SQLite transaction, then lends that transaction to Catalog. Head advance and
candidate attempt receipt are committed together by that caller; receipt failure
rolls back the head. The adapter never opens or commits a transaction or waits for
a lease. Duplicate receipts, readonly/unknown-schema errors, UNKNOWN/MISSING
metadata and fixture-only scope retain their existing handling.

This batch covers AssetHead only. Version/object/reference and feedback ownership
still use their existing library/storage/history modules. No production acceptance,
validation capability, schema migration or broader D1 completion is implied.

Hexagonal migration status: these Connection/Transaction functions are adapter-internal
APIs, not semantic application output ports. Production orchestration and the
compatibility Application facade still depend on this concrete adapter in this
first ownership-only batch. A later use-case separation must invoke a dedicated
acceptance-commit port whose SQLite implementation coordinates head CAS and the
attempt receipt internally in one transaction; database types must not cross that
semantic port, and Catalog must not depend on Production result types. This batch
does not claim to complete that separation.
