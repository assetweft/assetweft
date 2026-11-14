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

AssetHead and immutable version/reference ownership now reside in Catalog.
Object and feedback ownership still use storage/history modules. No production acceptance,
validation capability, schema migration or broader D1 completion is implied.

Hexagonal migration status: these Connection/Transaction functions remain
adapter-internal APIs. The Production acceptance use case depends on its own
semantic CommitAcceptance port; sqlite/acceptance coordinates Catalog CAS and the
Production attempt receipt internally in one transaction. The compatibility facade
still reads this adapter directly, and candidate initialization still borrows its
transaction. Staging and initialization separation remain future
batches; this does not claim overall DDD completion.


## Immutable metadata and exact references

`domain` owns Presence, ComponentRole, AssetVersion and ComponentReference and
pure validation/parsing. `library::*` re-exports the identical types, preserving
fields/derives and role strings. Presence remains UNKNOWN/MISSING only; metadata
never proves managed bytes, authorization or production readiness. Contract 1.0.0
and identity/hash/source/license validation remain exact and ordered as before.

The four `application` use cases register metadata versions, read exact versions,
register exact existing references and read ordered exact reference lists. They
use one core-owned Metadata capability covering this bounded metadata snapshot
purpose, with stable MetadataError outcomes. No SQL/rows/handles/vendor errors
cross the port. The shared Application facade assembles SqliteMetadata and calls
all four pure use cases; its public signatures and eight LibraryError categories
and original diagnostic payloads remain unchanged via exhaustive translations.

`sqlite_metadata` owns runtime SQL for aw_assets, aw_asset_versions and
aw_component_references. Version registration inserts asset+immutable version in
one checked short IMMEDIATE transaction. Reads keep deferred transactions and
schema checks, unknown stored-value rejection and exact ORDER BY
role,component_version_id. FK/uniqueness/immutability/frozen-reference guarantees
remain in unchanged migrations 001–006/schema6; this adapter never initializes
schema. Library still owns migration/connection infrastructure: the adapter's
concrete use of library::require_current and LibraryError is intentional migration
coupling, never a pure core dependency.

Version use-case validation precedes the adapter's retention lease acquisition.
Reference composition calls with_reference_lease BEFORE invoking the pure
reference use case, preserving existing Busy-versus-malformed-input precedence.
That constructor holds the existing storage Guard through validation and commit;
all other write-port calls also acquire the lease before SQLite IMMEDIATE.
The adapter never offers a successful unguarded write-port route and performs no
retry. No generic transaction callback or separate retention policy is added.

storage::commit retains validation before byte work and the existing lease,
publication and IMMEDIATE transaction boundary. It lends the checked connection
to Catalog's internal insert_version collaboration, which acquires no lease,
opens/commits no transaction, and must only be called inside this existing
lease+transaction boundary (or Catalog's checked registration transaction).
Object/version/managed-binding registration rollback remains one transaction.
The borrowed helper is concrete adapter collaboration, not a semantic core port.

library::validate/asset_version/component_references are explicit compatibility
forwards to Catalog for candidate snapshots, measurement/validation and acceptance
owner lookup. Their orchestration is unchanged. Managed-object ownership,
initialization/staging, feedback/history, Production workflow/budget/executor,
full port migration and overall DDD acceptance remain debt. Existing integration
tests are unchanged; pure use-case substitutes prove validation/delegation only,
while focused real-file tests check lease/error ordering and facade translations.
