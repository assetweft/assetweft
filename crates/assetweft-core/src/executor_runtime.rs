//! Responsibility: LOCAL TEST_FIXTURE bridge, not provider runtime admission.
//! Dependencies: Application budget once-claim and Executor checks; callers: local fixture tests.
//! Invariants: no conversion, automatic settlement, retries, credentials or transport.
//! Caller declarations are unauthenticated; arbitrary shell can bypass this policy.
//! Use existing workflow claim/run/finish APIs to schedule; query with Executor::query,
//! then supply a separate explicit Receipt to Application::reconcile_call. Neither
//! Succeeded nor cancellation establishes charge, no-charge, or billing stop.
use crate::{
    application::Application,
    budget::{Request, Unit},
    executor::{Backend, Error, Executor, Observation, Operation, SubmitRequest},
    library::LibraryError,
};

/// Pre-invocation failures never contain backend diagnostic payloads.
#[derive(Debug)]
pub enum PreflightError {
    Identity,
    Credentials,
    NotZeroAdditionalCost,
    Executor(Error),
    Database(LibraryError),
}

/// No conversion is attempted. The complete observation remains available on failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageSupport {
    Exact,
    UnsupportedScale,
    AmountOverflow,
}
fn usage_support(observation: &Observation) -> UsageSupport {
    for usage in &observation.usage {
        if matches!(&usage.unit, Unit::CurrencyMinor { decimals, .. } if *decimals > 9) {
            return UsageSupport::UnsupportedScale;
        }
        if usage.amount.is_some_and(|n| i64::try_from(n).is_err()) {
            return UsageSupport::AmountOverflow;
        }
    }
    UsageSupport::Exact
}

/// An invocation is not a successful execution or settlement. Inspect both results.
#[derive(Debug)]
pub enum Submission {
    NotInvoked,
    Invoked {
        backend: Result<Observation, Error>,
        /// UNKNOWN recording may fail after the side effect; ATTEMPTING remains non-replayable.
        recording: Result<(), LibraryError>,
        /// None means the checked executor returned an error, not zero usage.
        usage: Option<UsageSupport>,
    },
}

/// Executes only the declared local fixture path, once per immutable budget request.
/// Reservation/claim commit before the backend runs. A panic leaves ATTEMPTING.
/// # Errors
/// Returns pre-invocation validation/database errors. Post-invocation database errors
/// are returned alongside the original typed backend result in Submission::Invoked.
pub fn submit_local_test_fixture<B: Backend>(
    app: &mut Application,
    executor: &mut Executor<B>,
    budget: &Request,
    request: &SubmitRequest,
) -> Result<Submission, PreflightError> {
    if request.correlation.as_str() != budget.id
        || request.specification.as_str() != budget.input_fingerprint
    {
        return Err(PreflightError::Identity);
    }
    if request.credential.is_some() {
        return Err(PreflightError::Credentials);
    }
    if !executor.declaration().submit {
        return Err(PreflightError::Executor(Error::Unsupported(
            Operation::Submit,
        )));
    }
    // Existing allowance validation remains authoritative (including zero-capacity
    // ZeroAdditionalCost). Nonzero counts here are simulated fixture capacity.
    if budget.quote != Some(0) {
        return Err(PreflightError::NotZeroAdditionalCost);
    }
    app.reserve_call(budget).map_err(PreflightError::Database)?;
    let mut backend = None;
    let recorded = app.submit_test_fixture(&budget.id, || {
        backend = Some(executor.submit(request));
    });
    match backend {
        Some(backend) => {
            let usage = backend.as_ref().ok().map(usage_support);
            Ok(Submission::Invoked {
                backend,
                recording: recorded.map(|_| ()),
                usage,
            })
        }
        None => {
            recorded.map_err(PreflightError::Database)?;
            Ok(Submission::NotInvoked)
        }
    }
}
