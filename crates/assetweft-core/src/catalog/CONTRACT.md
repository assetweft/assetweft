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

The compatibility Application facade reads via the semantic HeadQuery use case. Candidate orchestration validates
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

Catalog owns AssetHead, immutable version/reference, managed objects and feedback
values. Connection/Transaction functions in `sqlite_metadata`, `sqlite_store` and
`objects_sqlite` remain adapter-internal APIs; they do not imply production
acceptance, validation capability or schema migration. The Production acceptance
use case depends on its own semantic CommitAcceptance port; `sqlite/acceptance`
coordinates Catalog CAS and the Production attempt receipt internally in one
transaction. Application reads heads via HeadQuery and assembles the Catalog
registration/reference use cases directly; initialization still borrows the
caller's transaction. No overall DDD or product acceptance is claimed.


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

Application assembles `catalog::application` use cases with the concrete
`SqliteMetadata` adapter directly; `library` retains only migration/schema,
LibraryError and compatibility type re-exports. Candidate snapshots,
measurement/validation and acceptance owner lookup call the owner adapters
(`sqlite_metadata`, `sqlite_store`, `head_sqlite`) without private forwards.
Existing integration tests are unchanged; pure use-case substitutes prove
validation/delegation only, while focused real-file tests check lease/error
ordering and facade translations.

## Exact-version feedback

Catalog domain owns Decision, Feedback, FeedbackRecord, HistoryQuery and their
existing constants and pure ordered validation. Legacy history paths re-export
the identical values. feedback::{append,history,latest} depend only on the
purpose-owned Feedback capability and domain values; Application composes
SqliteFeedback and translates the existing stable MetadataError categories
losslessly to LibraryError. No connection or vendor error crosses this port.

sqlite_feedback owns unchanged INSERT/SELECT, parameters, row decoding, schema
check and transaction lifecycle. Append validation precedes IMMEDIATE and schema
check; exact asset/version existence remains enforced atomically by migration
006's binding trigger. Append-only/update/delete triggers and sequence semantics
are unchanged. History uses a deferred checked transaction, descending local
sequence and strict before cursor. latest is exactly history with limit one.
Migration/connection infrastructure remains in library/sqlite; this is disclosed
adapter coupling. Opinions never imply candidate acceptance, rights or readiness.


## Managed objects and retention

Catalog objects_domain owns the identical StoredObject/Collection values, strict
hash/limit/completeness/digest policies and protected-collection decision.
storage re-exports these values; StorageError retains original Io/Library payloads.
objects_application owns commit/managed/protect/collect. ManagedObjects is a narrow
publication/retention capability with opaque failure and staged receipt associated
types: core never imports paths, readers, SQLite or SDK errors, and never examines
an adapter failure. Adapter errors pass through unchanged, preserving diagnostics.
Commit validates metadata then byte limits, stages bounded verified bytes, then
publishes/registers. A failed stage cannot register. Managed metadata absence is None.

objects_sqlite captures connection/root/stream at composition. It owns original
object SQL, bounded temp streaming, actual SHA256, full reread and hard_link publish
before short checked IMMEDIATE registration. Object/version/managed links share
the existing Catalog borrowed insert helper and one transaction. No nested lease
or commit is added. The sole storage Guard/reference_guard/bind implementation
remains shared by metadata, backup and candidate adapters. GC/protect take that
lease before IMMEDIATE; GC invokes pure collection policy on the original current
SQL protection predicate inside this transaction. Referenced/pinned/default unique
objects stay protected. SQL commit precedes unlink with the lease still held;
failures may leave an orphan, never grant missing bytes readiness. No stale-lock
breaking, schema change, retry or deletion scope expansion is introduced.

The unchanged Application methods assemble the `objects_application` use cases
with the concrete `objects_sqlite` adapter. Concrete storage errors/lease/binding
remain infrastructure, not core ports. Current-head CI, independent review,
crash/power-loss/Windows and real-asset acceptance are root-owned and not
established by these contract notes.

## Protected backup and fresh restore

backup_domain owns the identical public Limits, Progress, Object and Manifest
values (legacy backup::* re-exports), bounded byte/identity policy and format
1/schema 6 validation. backup_application uses ProtectedBackup and FreshRestore
semantic capabilities: protected snapshot, exact registered-object copy, then
manifest publication; restore reads/validates the manifest, verifies the closed
snapshot and registered set, copies into a fresh target, then publishes completion.
No path, connection, transaction, file or IO dependency crosses these ports.
Associated errors preserve the original public BackupError diagnostic payloads.

backup_sqlite captures paths/connections/callbacks, retains the single existing
reference_guard through snapshot/copy/publication, and uses the existing bind.
SQLite stepping, schema/integrity checks, snapshot object query, sidecar rejection,
chunked hashing/copy, progress/deadline order, owner isolation and no-clobber
publication remain concrete. The clock starts at the original boundary; no
transaction spans filesystem copy or callback. Failed/cancelled operations leave
no successful manifest. Metadata-only UNKNOWN/MISSING does not become ready bytes.

head_application validates exact asset identity before HeadQuery. head_sqlite owns
the original schema guard, SELECT and contract decoding; borrowed sqlite_store
current forwards through the same use case. initialize/advance remain concrete
crate-private transaction APIs consumed by the Production acceptance and
initialization use cases under their existing lease+transaction boundaries. No
dependency/schema/SQL migration or new gate.
