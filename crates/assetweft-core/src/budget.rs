//! Responsibility: legacy budget API compatibility and concrete composition.
//! Dependencies: Consumption use cases and SQLite adapter; Application/executor consume identical types.
//! Invariants: no business policy here; all callbacks run through the Consumption use case.
pub use crate::consumption::domain::{
    Allowance, Approval, Balance, Intent, Receipt, Request, Resolution, State, Unit, Usage,
};
use crate::{
    consumption::{application, sqlite::SqliteBudgetLedger},
    library::LibraryError,
};
use rusqlite::Connection;
pub(crate) fn approve(c: &mut Connection, a: &Allowance) -> Result<(), LibraryError> {
    application::approve(&mut SqliteBudgetLedger(c), a).map_err(Into::into)
}
pub(crate) fn balance(c: &Connection, id: &str) -> Result<Balance, LibraryError> {
    application::balance(&mut crate::consumption::sqlite::BudgetQuery(c), id).map_err(Into::into)
}
pub(crate) fn reserve(c: &mut Connection, r: &Request) -> Result<Intent, LibraryError> {
    application::reserve(&mut SqliteBudgetLedger(c), r).map_err(Into::into)
}
pub(crate) fn intent(c: &Connection, id: &str) -> Result<Option<Intent>, LibraryError> {
    application::intent(&mut crate::consumption::sqlite::BudgetQuery(c), id).map_err(Into::into)
}
pub(crate) fn cancel(c: &mut Connection, id: &str) -> Result<(), LibraryError> {
    application::cancel(&mut SqliteBudgetLedger(c), id).map_err(Into::into)
}
pub(crate) fn reconcile(c: &mut Connection, id: &str, r: &Receipt) -> Result<(), LibraryError> {
    application::reconcile(&mut SqliteBudgetLedger(c), id, r).map_err(Into::into)
}
pub(crate) fn receipts(c: &Connection, id: &str) -> Result<Vec<Receipt>, LibraryError> {
    application::receipts(&mut crate::consumption::sqlite::BudgetQuery(c), id).map_err(Into::into)
}
pub(crate) fn submit_test_fixture(
    c: &mut Connection,
    id: &str,
    callback: impl FnOnce(),
) -> Result<bool, LibraryError> {
    application::submit_test_fixture(&mut SqliteBudgetLedger(c), id, callback).map_err(Into::into)
}
