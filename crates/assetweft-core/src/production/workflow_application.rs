//! Responsibility: actual workflow command/query and claim → driver → finish use cases.
//! Dependencies: pure workflow domain and semantic WorkflowRuns capability.
//! Callers: compatibility composition; driver runs after durable claim, never replayed.
use super::{
    workflow_domain::{self, Outcome, Step, StepRecord},
    workflow_ports::{WorkflowQuery, WorkflowRuns},
};

pub(crate) fn create<P: WorkflowRuns>(
    port: &mut P,
    run: &str,
    steps: &[Step],
) -> Result<(), P::Error> {
    workflow_domain::validate_graph(run, steps)?;
    port.create(run, steps)
}
pub(crate) fn claim<P: WorkflowRuns>(port: &mut P, run: &str) -> Result<Option<String>, P::Error> {
    port.claim(run)
}
pub(crate) fn finish<P: WorkflowRuns>(
    port: &mut P,
    run: &str,
    id: &str,
    outcome: &Outcome,
) -> Result<(), P::Error> {
    workflow_domain::validate_outcome(outcome)?;
    port.finish(run, id, outcome)
}
pub(crate) fn cancel<P: WorkflowRuns>(port: &mut P, run: &str) -> Result<(), P::Error> {
    port.cancel(run)
}
pub(crate) fn read<P: WorkflowQuery>(port: &P, run: &str) -> Result<Vec<StepRecord>, P::Error> {
    port.read(run)
}
pub(crate) fn is_cancelled<P: WorkflowQuery>(port: &P, run: &str) -> Result<bool, P::Error> {
    port.is_cancelled(run)
}
pub(crate) fn run<P: WorkflowRuns>(
    port: &mut P,
    run: &str,
    driver: impl FnOnce(&str) -> Outcome,
) -> Result<bool, P::Error> {
    let Some(id) = claim(port, run)? else {
        return Ok(false);
    };
    let outcome = driver(&id);
    finish(port, run, &id, &outcome)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::super::workflow_domain::PolicyError;
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    struct Runs {
        events: Rc<RefCell<Vec<&'static str>>>,
        claimed: bool,
        record_failure: bool,
    }
    impl WorkflowQuery for Runs {
        type Error = PolicyError;
        fn read(&self, _: &str) -> Result<Vec<StepRecord>, PolicyError> {
            Ok(vec![])
        }
        fn is_cancelled(&self, _: &str) -> Result<bool, PolicyError> {
            Ok(false)
        }
    }
    impl WorkflowRuns for Runs {
        fn create(&mut self, _: &str, _: &[Step]) -> Result<(), PolicyError> {
            self.events.borrow_mut().push("create");
            Ok(())
        }
        fn claim(&mut self, _: &str) -> Result<Option<String>, PolicyError> {
            self.events.borrow_mut().push("claim_committed");
            if self.claimed {
                return Ok(None);
            }
            self.claimed = true;
            Ok(Some("a".into()))
        }
        fn finish(&mut self, _: &str, _: &str, _: &Outcome) -> Result<(), PolicyError> {
            self.events.borrow_mut().push("finish");
            if self.record_failure {
                return Err(PolicyError("record failed".into()));
            }
            self.claimed = false;
            Ok(())
        }
        fn cancel(&mut self, _: &str) -> Result<(), PolicyError> {
            Ok(())
        }
    }
    fn port() -> Runs {
        Runs {
            events: Rc::new(RefCell::new(vec![])),
            claimed: false,
            record_failure: false,
        }
    }
    fn success() -> Outcome {
        Outcome::Success {
            raw: "actual result".into(),
            evidence: None,
        }
    }

    #[test]
    fn claim_driver_finish_order_and_record_failure_never_replays() {
        let mut p = port();
        p.record_failure = true;
        let events = p.events.clone();
        assert_eq!(
            run(&mut p, "r", |id| {
                assert_eq!(id, "a");
                assert_eq!(&*events.borrow(), &["claim_committed"]);
                events.borrow_mut().push("driver");
                success()
            }),
            Err(PolicyError("record failed".into()))
        );
        assert_eq!(
            &*p.events.borrow(),
            &["claim_committed", "driver", "finish"]
        );
        assert!(p.claimed);
        assert!(
            !run(&mut p, "r", |_| panic!(
                "unresolved claim must not invoke driver"
            ))
            .unwrap()
        );
    }
    #[test]
    fn callback_panic_leaves_claim_unresolved_without_finish() {
        let mut p = port();
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = run(&mut p, "r", |_| panic!("interruption"));
            }))
            .is_err()
        );
        assert!(p.claimed);
        assert_eq!(&*p.events.borrow(), &["claim_committed"]);
    }
    #[test]
    fn invalid_graph_and_oversized_outcome_never_reach_persistence() {
        let mut p = port();
        assert!(
            create(
                &mut p,
                "r",
                &[Step {
                    id: "a".into(),
                    dependencies: vec!["a".into()]
                }]
            )
            .is_err()
        );
        assert!(
            finish(
                &mut p,
                "r",
                "a",
                &Outcome::Failure {
                    raw: "x".repeat(1_048_577),
                    evidence: None
                }
            )
            .is_err()
        );
        assert!(p.events.borrow().is_empty());
    }
}
