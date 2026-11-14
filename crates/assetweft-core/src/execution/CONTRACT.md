# Execution existing protocol and LOCAL TEST_FIXTURE

`domain` owns the SAME Reference, declarations, requests, observations, artifacts,
errors and state types explicitly re-exported by legacy `executor`; `ports::Backend`
is the purpose-owned synchronous protocol capability. `application::Executor`
snapshots declarations and checks support before backend side effects, validates
observations at protocol scale 0..18 and retains credential/usage Debug redaction.
Consumption Unit/Usage are imported from their actual owning domain, never copied.

`runtime_application` owns local fixture identity -> no credential -> submit
support -> quote Some(0) -> reserve -> once-submit sequencing. `ports::BudgetOnce`
exposes only reserve and once callback semantics with an opaque associated error.
The concrete Consumption adapter invokes its accepted reserve and submit_test_fixture
use cases through SqliteBudgetLedger; it does not implement money, claim or UNKNOWN
policy. Consumption never imports Execution. The callback follows committed claim;
panic leaves ATTEMPTING, replay never invokes backend. Backend Result and UNKNOWN
recording Result remain separate; NotInvoked never contains a fabricated result.

Generic core PreflightError<E>/Submission<E> preserve opaque diagnostic identity;
legacy executor_runtime exposes their concrete LibraryError aliases. UsageSupport
is the SAME core type. Root Application's existing submit method assembles its
private connection with the adapter; the legacy free function forwards to it.
Core has no RootApplication/LibraryError/rusqlite/filesystem/transport imports.
No tables are owned here, and no schema/SQL/dependency changes are required.

Actual consumers: root Application submit_executor_local_test_fixture and the
legacy executor_runtime free function; existing executor/executor_runtime tests
exercise those paths including real-file budget/workflow assembly. Consumption's
own domain/use cases remain authoritative for allowance/identity/claim policies.
Legacy `executor`/`executor_runtime` re-export the SAME items; Application
assembles the concrete adapter directly.

Quote zero is unauthenticated fixture declaration, not provider admission or rights.
Protocol scale >9 is explicitly UnsupportedScale for ledger use, overflow remains
AmountOverflow, None usage stays unknown. No rounding, settlement, retry, download,
model/provider/auth/paid behavior is added. Issue #28 remains OPEN; affected
workflow/budget/CLI CI, review and whole product acceptance remain root-owned.
