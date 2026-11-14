//! Responsibility: pure LOCAL TEST_FIXTURE preflight and execution orchestration.
//! Dependencies: Execution domain/ports and Consumption request values only.
//! Invariants: preflight -> reserve -> once callback; backend and recording results stay separate.
use super::{
    application::Executor,
    domain::*,
    ports::{Backend, BudgetOnce},
};
use crate::consumption::domain::{Request, Unit};

/// Pre-invocation failures never contain backend diagnostic payloads.
#[derive(Debug)]
pub enum PreflightError<E> {
    Identity,
    Credentials,
    NotZeroAdditionalCost,
    Executor(Error),
    Database(E),
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
pub enum Submission<E> {
    NotInvoked,
    Invoked {
        backend: Result<Observation, Error>,
        /// UNKNOWN recording may fail after the side effect; ATTEMPTING remains non-replayable.
        recording: Result<(), E>,
        /// None means the checked executor returned an error, not zero usage.
        usage: Option<UsageSupport>,
    },
}

/// Executes only the declared local fixture path, once per immutable budget request.
/// Reservation/claim commit before the backend runs. A panic leaves ATTEMPTING.
/// # Errors
/// Returns pre-invocation validation/database errors. Post-invocation database errors
/// are returned alongside the original typed backend result in Submission::Invoked.
pub fn submit_local_test_fixture<B: Backend, P: BudgetOnce>(
    budget_once: &mut P,
    executor: &mut Executor<B>,
    budget: &Request,
    request: &SubmitRequest,
) -> Result<Submission<P::Error>, PreflightError<P::Error>> {
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
    budget_once
        .reserve(budget)
        .map_err(PreflightError::Database)?;
    let mut backend = None;
    let recorded = budget_once.submit_once(&budget.id, || {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, rc::Rc};

    struct Budget(Rc<RefCell<Vec<&'static str>>>, bool);
    impl BudgetOnce for Budget {
        type Error = &'static str;
        fn reserve(&mut self, _: &Request) -> Result<(), Self::Error> {
            self.0.borrow_mut().push("reserve");
            Ok(())
        }
        fn submit_once(&mut self, _: &str, callback: impl FnOnce()) -> Result<bool, Self::Error> {
            self.0.borrow_mut().push("once");
            if !self.1 {
                return Ok(false);
            }
            callback();
            Err("recording diagnostic")
        }
    }
    struct Local(Rc<RefCell<Vec<&'static str>>>);
    impl Backend for Local {
        fn declaration(&self) -> Declaration {
            Declaration {
                backend: reference("local"),
                backend_version: reference("1"),
                profile: reference("fixture"),
                profile_version: reference("1"),
                submit: true,
                query: false,
                download: false,
                cancel: CancelCapability::Unsupported,
            }
        }
        fn submit(&mut self, _: &SubmitRequest) -> Result<Observation, Error> {
            self.0.borrow_mut().push("backend");
            Err(Error::InvalidUsage)
        }
        fn query(&mut self, _: &Reference) -> Result<Observation, Error> {
            unreachable!()
        }
        fn download(&mut self, _: &Reference) -> Result<Artifact, Error> {
            unreachable!()
        }
        fn cancel(&mut self, _: &Reference) -> Result<CancelResult, Error> {
            unreachable!()
        }
    }
    fn reference(s: &str) -> Reference {
        Reference::new(s).unwrap()
    }

    #[test]
    fn semantic_port_preserves_order_noninvocation_and_both_diagnostics() {
        let events = Rc::new(RefCell::new(vec![]));
        let mut budget_once = Budget(events.clone(), true);
        let mut executor = Executor::new(Local(events.clone()));
        let budget = Request {
            id: "call".into(),
            account: "account".into(),
            input_fingerprint: "spec".into(),
            unit: Unit::Token,
            quote: Some(0),
            reservation: 1,
            source: "fixture".into(),
        };
        let mut request = SubmitRequest {
            correlation: reference("wrong"),
            specification: reference("spec"),
            credential: None,
        };
        assert!(matches!(
            submit_local_test_fixture(&mut budget_once, &mut executor, &budget, &request),
            Err(PreflightError::Identity)
        ));
        assert!(events.borrow().is_empty());
        request.correlation = reference("call");
        assert!(matches!(
            submit_local_test_fixture(&mut budget_once, &mut executor, &budget, &request),
            Ok(Submission::Invoked {
                backend: Err(Error::InvalidUsage),
                recording: Err("recording diagnostic"),
                usage: None
            })
        ));
        assert_eq!(*events.borrow(), ["reserve", "once", "backend"]);
        events.borrow_mut().clear();
        budget_once.1 = false;
        assert!(matches!(
            submit_local_test_fixture(&mut budget_once, &mut executor, &budget, &request),
            Ok(Submission::NotInvoked)
        ));
        assert_eq!(*events.borrow(), ["reserve", "once"]);
    }
}
