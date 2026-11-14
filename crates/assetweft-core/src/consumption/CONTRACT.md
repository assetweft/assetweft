# Consumption local budget boundary

Domain owns Allowance, Request, Intent, Balance, Receipt, Usage, Unit, Approval,
State and Resolution. `budget` explicitly re-exports the SAME types, preserving
serde and executor compatibility. Approval/source/fingerprint/quote are caller
local declarations, never authentication, provider idempotency or rights proof.

Application validates commands, delegates one atomic ledger operation, and owns
claim -> callback -> unknown sequencing. Callback runs after committed claim,
never within a transaction; a panic retains ATTEMPTING. No retry/replay exists.
Core BudgetLedger and BudgetQueries expose semantic values and BudgetError;
no connection, rowcount, path or Application crosses either capability.

SQLite owns aw_budget_accounts, aw_budget_intents, aw_budget_receipts and their
SQL/encoding/lifecycle. Each mutation retains its original IMMEDIATE transaction,
schema checks, conditional writes and rollback. Domain reservation policy runs
inside that transaction on the loaded balance; conditional capacity UPDATE stays
as the persistence guard. Immutable identity decisions precede writes. Exact
integer i64 bounds/units/scales and None quote are preserved. PREPARING cancel
settles/releases exactly once without creating a receipt. ATTEMPTING/UNKNOWN
cancel retains reservation. Reconciliation checks prior exact receipt before
uncertain state, and atomically settles plus appends explicit receipt. Charge
above reservation fails without release. Original SQL strings/parameters/order
and error messages/categories survive; errors map losslessly at composition.

Legacy `budget` re-exports the SAME domain values; Application retains its
signatures/connection ownership and assembles the `application` use cases with
`sqlite::SqliteBudgetLedger`/`BudgetQuery` directly. Execution's
`consumption_adapter` invokes the accepted reserve and submit_test_fixture use
cases on the same adapter; it does not duplicate money, claim or UNKNOWN policy.
No schema/dependency/product/provider/auth/paid changes. Existing file integration
tests remain untouched; pure tests prove policy and orchestration only.
