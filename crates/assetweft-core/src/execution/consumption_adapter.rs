//! Responsibility: concrete Consumption assembly for Execution local fixtures.
//! Dependencies: accepted Consumption use cases/SQLite adapter and legacy LibraryError.
//! Callers: root Application; invariants: no duplicated money/claim/UNKNOWN policy.
use super::ports::BudgetOnce;
use crate::{
    consumption::{application, domain::Request, sqlite::SqliteBudgetLedger},
    library::LibraryError,
};
use rusqlite::Connection;

pub(crate) struct ConsumptionBudgetOnce<'a>(pub(crate) &'a mut Connection);
impl BudgetOnce for ConsumptionBudgetOnce<'_> {
    type Error = LibraryError;
    fn reserve(&mut self, request: &Request) -> Result<(), Self::Error> {
        application::reserve(&mut SqliteBudgetLedger(self.0), request)
            .map(|_| ())
            .map_err(Into::into)
    }
    fn submit_once(&mut self, id: &str, callback: impl FnOnce()) -> Result<bool, Self::Error> {
        application::submit_test_fixture(&mut SqliteBudgetLedger(self.0), id, callback)
            .map_err(Into::into)
    }
}

/// Concrete legacy error instantiations; core retains the exact opaque diagnostic payload.
pub type PreflightError = super::runtime_application::PreflightError<LibraryError>;
pub type Submission = super::runtime_application::Submission<LibraryError>;
