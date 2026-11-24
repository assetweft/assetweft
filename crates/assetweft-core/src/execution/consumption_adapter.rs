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

/// Retains the typed LibraryError for callers that match executor_runtime::PreflightError variants.
#[derive(Debug)]
pub enum PreflightError {
    Identity,
    Credentials,
    NotZeroAdditionalCost,
    Executor(super::domain::Error),
    Database(LibraryError),
}

/// Preserves typed backend and UNKNOWN-recording results independently.
#[derive(Debug)]
pub enum Submission {
    NotInvoked,
    Invoked {
        backend: Result<super::domain::Observation, super::domain::Error>,
        recording: Result<(), LibraryError>,
        usage: Option<super::runtime_application::UsageSupport>,
    },
}

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
