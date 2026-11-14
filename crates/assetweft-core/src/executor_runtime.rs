//! Responsibility: legacy concrete LOCAL TEST_FIXTURE composition boundary.
//! Dependencies: root Application assembly and Execution core; callers: legacy clients.
//! Invariants: no business policy here; original LibraryError payloads preserved.
pub use crate::execution::{
    consumption_adapter::{PreflightError, Submission},
    runtime_application::UsageSupport,
};
use crate::{
    application::Application,
    consumption::domain::Request,
    executor::{Backend, Executor, SubmitRequest},
};

/// Executes the declared local fixture through Execution and Consumption, once.
/// # Errors
/// Returns preflight failures; recording failures retain the typed backend result.
pub fn submit_local_test_fixture<B: Backend>(
    app: &mut Application,
    executor: &mut Executor<B>,
    budget: &Request,
    request: &SubmitRequest,
) -> Result<Submission, PreflightError> {
    app.submit_executor_local_test_fixture(executor, budget, request)
}
