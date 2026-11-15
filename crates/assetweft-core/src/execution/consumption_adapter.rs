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

/// Concrete legacy preflight failure: a real public enum, as before generic core existed.
/// Variants, payloads and the typed `LibraryError` database error are unchanged, so
/// callers can import `executor_runtime::PreflightError::Identity` directly.
#[derive(Debug)]
pub enum PreflightError {
    Identity,
    Credentials,
    NotZeroAdditionalCost,
    Executor(super::domain::Error),
    Database(LibraryError),
}

/// Concrete legacy invocation outcome: a real public enum, as before generic core existed.
/// Backend and UNKNOWN-recording results stay separate and typed; `NotInvoked` never
/// fabricates a backend result.
#[derive(Debug)]
pub enum Submission {
    NotInvoked,
    Invoked {
        backend: Result<super::domain::Observation, super::domain::Error>,
        recording: Result<(), LibraryError>,
        usage: Option<super::runtime_application::UsageSupport>,
    },
}

/// Exhaustive, value-preserving translation at the legacy boundary; no policy lives here.
impl From<super::runtime_application::PreflightError<LibraryError>> for PreflightError {
    fn from(value: super::runtime_application::PreflightError<LibraryError>) -> Self {
        use super::runtime_application::PreflightError as Generic;
        match value {
            Generic::Identity => Self::Identity,
            Generic::Credentials => Self::Credentials,
            Generic::NotZeroAdditionalCost => Self::NotZeroAdditionalCost,
            Generic::Executor(error) => Self::Executor(error),
            Generic::Database(error) => Self::Database(error),
        }
    }
}

/// Exhaustive, value-preserving translation at the legacy boundary; no policy lives here.
impl From<super::runtime_application::Submission<LibraryError>> for Submission {
    fn from(value: super::runtime_application::Submission<LibraryError>) -> Self {
        use super::runtime_application::Submission as Generic;
        match value {
            Generic::NotInvoked => Self::NotInvoked,
            Generic::Invoked {
                backend,
                recording,
                usage,
            } => Self::Invoked {
                backend,
                recording,
                usage,
            },
        }
    }
}
