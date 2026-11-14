//! Responsibility: purpose-owned atomic budget/intent/receipt ledger capability.
//! Dependencies: Consumption domain only; use cases consume it, SQLite implements it.
//! Invariants: each mutation is one atomic decision/commit; no SQL counters escape.
use super::domain::*;
pub(crate) trait BudgetLedger {
    fn approve(&mut self, a: &Allowance) -> Result<()>;
    fn reserve(&mut self, r: &Request) -> Result<Intent>;
    fn claim(&mut self, id: &str) -> Result<bool>;
    fn unknown(&mut self, id: &str) -> Result<()>;
    fn cancel(&mut self, id: &str) -> Result<()>;
    fn reconcile(&mut self, id: &str, r: &Receipt) -> Result<()>;
}
pub(crate) trait BudgetQueries {
    fn balance(&mut self, id: &str) -> Result<Balance>;
    fn intent(&mut self, id: &str) -> Result<Option<Intent>>;
    fn receipts(&mut self, id: &str) -> Result<Vec<Receipt>>;
}
