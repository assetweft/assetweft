//! LOCAL TEST_FIXTURE seam gates: persistent Application, counted in-process backend.
//! No provider, authentication, money, network, artifact transport or model invocation.
use assetweft_core::{
    application::Application, budget::*, executor::*, executor_runtime::*, workflow,
};
use std::{cell::Cell, path::Path, rc::Rc};

struct Fixture {
    count: Rc<Cell<usize>>,
    query_count: Rc<Cell<usize>>,
    answer: Result<Observation, Error>,
    submit: bool,
    action: Option<Box<dyn FnMut()>>,
}
fn reference(s: &str) -> Reference {
    Reference::new(s).unwrap()
}
impl Backend for Fixture {
    fn declaration(&self) -> Declaration {
        Declaration {
            backend: reference("local"),
            backend_version: reference("1"),
            profile: reference("fixture"),
            profile_version: reference("1"),
            submit: self.submit,
            query: true,
            download: false,
            cancel: CancelCapability::Confirmation,
        }
    }
    fn submit(&mut self, _: &SubmitRequest) -> Result<Observation, Error> {
        self.count.set(self.count.get() + 1);
        if let Some(action) = &mut self.action {
            action();
        }
        self.answer.clone()
    }
    fn query(&mut self, _: &Reference) -> Result<Observation, Error> {
        self.query_count.set(self.query_count.get() + 1);
        self.answer.clone()
    }
    fn cancel(&mut self, _: &Reference) -> Result<CancelResult, Error> {
        Ok(CancelResult::Confirmed)
    }
    fn download(&mut self, _: &Reference) -> Result<Artifact, Error> {
        panic!("not supported")
    }
}
fn fixture(answer: Result<Observation, Error>) -> Fixture {
    Fixture {
        count: Rc::new(Cell::new(0)),
        query_count: Rc::new(Cell::new(0)),
        answer,
        submit: true,
        action: None,
    }
}
fn observation(state: RemoteState, usage: Vec<Usage>) -> Observation {
    Observation {
        state,
        job: Some(reference("job")),
        usage,
    }
}
fn setup(path: &Path) -> Application {
    let mut a = Application::open(path).unwrap();
    a.initialize_library().unwrap();
    a.approve_budget(&Allowance {
        id: "a".into(),
        unit: Unit::Token,
        capacity: 100,
        approval: Approval::ExplicitBounded,
        source: "TEST_FIXTURE declaration".into(),
    })
    .unwrap();
    a
}
fn request() -> (Request, SubmitRequest) {
    (
        Request {
            id: "call".into(),
            account: "a".into(),
            input_fingerprint: "spec".into(),
            unit: Unit::Token,
            quote: Some(0),
            reservation: 10,
            source: "TEST_FIXTURE".into(),
        },
        SubmitRequest {
            correlation: reference("call"),
            specification: reference("spec"),
            credential: None,
        },
    )
}
fn invoke(
    a: &mut Application,
    e: &mut Executor<Fixture>,
    b: &Request,
    r: &SubmitRequest,
) -> Submission {
    a.submit_executor_local_test_fixture(e, b, r).unwrap()
}
#[test]
fn once_only_error_unknown_success_panic_and_reopen() {
    for mode in 0..4 {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("db");
        let mut a = setup(&p);
        let answer = if mode == 0 {
            Err(Error::InvalidUsage)
        } else {
            Ok(observation(
                if mode == 1 {
                    RemoteState::Unknown
                } else {
                    RemoteState::Succeeded
                },
                vec![],
            ))
        };
        let mut f = fixture(answer.clone());
        let count = f.count.clone();
        if mode == 3 {
            f.action = Some(Box::new(|| panic!("fixture interruption")));
        }
        let mut e = Executor::new(f);
        let (b, r) = request();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            invoke(&mut a, &mut e, &b, &r)
        }));
        if mode == 3 {
            assert!(result.is_err());
        } else {
            let Submission::Invoked {
                backend, recording, ..
            } = result.unwrap()
            else {
                panic!("missing attempt")
            };
            assert_eq!(backend, answer);
            recording.unwrap();
        }
        drop(a);
        let mut a = Application::open(&p).unwrap();
        assert!(matches!(
            invoke(&mut a, &mut e, &b, &r),
            Submission::NotInvoked
        ));
        assert_eq!(count.get(), 1);
        assert_eq!(
            a.call_intent(&b.id).unwrap().unwrap().state,
            if mode == 3 {
                State::Attempting
            } else {
                State::Unknown
            }
        );
        assert_eq!(a.call_intent(&b.id).unwrap().unwrap().charge, None);
        assert!(a.call_receipts(&b.id).unwrap().is_empty());
        assert_eq!(
            e.cancel(&reference("job")).unwrap(),
            CancelResult::Confirmed
        );
        a.cancel_call(&b.id).unwrap();
        assert_eq!(a.budget_balance("a").unwrap().reserved, 10);
        assert!(matches!(
            invoke(&mut a, &mut e, &b, &r),
            Submission::NotInvoked
        ));
        assert_eq!(count.get(), 1);
    }
}
#[test]
fn preflight_and_cancelled_preparing_never_invoke() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("db");
    let mut a = setup(&p);
    let f = fixture(Ok(observation(RemoteState::Unknown, vec![])));
    let count = f.count.clone();
    let mut e = Executor::new(f);
    let (b, r) = request();
    let mut bad = r.clone();
    bad.specification = reference("wrong");
    assert!(matches!(
        a.submit_executor_local_test_fixture(&mut e, &b, &bad),
        Err(PreflightError::Identity)
    ));
    bad = r.clone();
    bad.correlation = reference("wrong");
    assert!(
        a.submit_executor_local_test_fixture(&mut e, &b, &bad)
            .is_err()
    );
    bad = r.clone();
    bad.credential = Some(CredentialReference::OfficialStore(reference("opaque")));
    assert!(
        a.submit_executor_local_test_fixture(&mut e, &b, &bad)
            .is_err()
    );
    for decimals in 10..=18 {
        let mut b = b.clone();
        b.unit = Unit::CurrencyMinor {
            currency: "USD".into(),
            decimals,
        };
        assert!(
            a.submit_executor_local_test_fixture(&mut e, &b, &r)
                .is_err()
        );
    }
    let mut big = b.clone();
    big.reservation = u64::MAX;
    assert!(
        a.submit_executor_local_test_fixture(&mut e, &big, &r)
            .is_err()
    );
    let mut f = fixture(Ok(observation(RemoteState::Unknown, vec![])));
    f.submit = false;
    let unsupported_count = f.count.clone();
    assert!(
        a.submit_executor_local_test_fixture(&mut Executor::new(f), &b, &r)
            .is_err()
    );
    assert_eq!(unsupported_count.get(), 0);
    a.reserve_call(&b).unwrap();
    a.cancel_call(&b.id).unwrap();
    drop(a);
    let mut a = Application::open(&p).unwrap();
    assert!(matches!(
        invoke(&mut a, &mut e, &b, &r),
        Submission::NotInvoked
    ));
    assert_eq!(count.get(), 0);
}
#[test]
fn precision_observations_are_lossless_and_never_settle_implicitly() {
    for decimals in 0..=18 {
        for amount in [None, Some(17), Some(i64::MAX as u64), Some(u64::MAX)] {
            let d = tempfile::tempdir().unwrap();
            let mut a = setup(&d.path().join("db"));
            let original = observation(
                RemoteState::Succeeded,
                vec![Usage {
                    unit: Unit::CurrencyMinor {
                        currency: "USD".into(),
                        decimals,
                    },
                    amount,
                    source: "fixture-meter".into(),
                }],
            );
            let mut e = Executor::new(fixture(Ok(original.clone())));
            let (b, r) = request();
            let Submission::Invoked {
                backend,
                recording,
                usage,
            } = invoke(&mut a, &mut e, &b, &r)
            else {
                panic!("not invoked")
            };
            assert_eq!(backend.unwrap(), original);
            recording.unwrap();
            assert_eq!(
                usage,
                Some(if decimals > 9 {
                    UsageSupport::UnsupportedScale
                } else if amount == Some(u64::MAX) {
                    UsageSupport::AmountOverflow
                } else {
                    UsageSupport::Exact
                })
            );
            assert_eq!(a.budget_balance("a").unwrap().reserved, 10);
            assert_eq!(a.call_intent(&b.id).unwrap().unwrap().charge, None);
            let q = Receipt {
                id: "q".into(),
                source: "explicit local receipt".into(),
                resolution: Resolution::Unresolved,
                usage: original.usage.clone(),
            };
            if usage == Some(UsageSupport::Exact) {
                a.reconcile_call(&b.id, &q).unwrap();
                assert_eq!(a.call_receipts(&b.id).unwrap(), vec![q]);
            } else {
                assert!(a.reconcile_call(&b.id, &q).is_err());
                assert!(a.call_receipts(&b.id).unwrap().is_empty());
            }
        }
    }
}
#[test]
fn explicit_query_receipt_identity_and_exact_settlement() {
    let d = tempfile::tempdir().unwrap();
    let mut a = setup(&d.path().join("db"));
    let f = fixture(Ok(observation(
        RemoteState::Succeeded,
        vec![Usage {
            unit: Unit::Token,
            amount: Some(3),
            source: "local-meter".into(),
        }],
    )));
    let queries = f.query_count.clone();
    let mut e = Executor::new(f);
    let (b, r) = request();
    invoke(&mut a, &mut e, &b, &r);
    let observed = e.query(&reference("job")).unwrap();
    assert_eq!(queries.get(), 1);
    assert_eq!(a.budget_balance("a").unwrap().reserved, 10);
    let q = Receipt {
        id: "query-1".into(),
        source: "explicit local evidence".into(),
        resolution: Resolution::Confirmed { charge: 3 },
        usage: observed.usage,
    };
    assert!(a.reconcile_call("wrong-call", &q).is_err());
    a.reconcile_call(&b.id, &q).unwrap();
    a.reconcile_call(&b.id, &q).unwrap();
    let mut conflict = q.clone();
    conflict.usage[0].amount = Some(4);
    assert!(a.reconcile_call(&b.id, &conflict).is_err());
    assert_eq!(a.call_receipts(&b.id).unwrap(), vec![q]);
    let balance = a.budget_balance("a").unwrap();
    assert_eq!((balance.reserved, balance.spent), (0, 3));
}
#[test]
fn second_application_writes_and_recording_failure_keeps_result() {
    for fail in [false, true] {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("db");
        let mut a = setup(&p);
        let action_path = p.clone();
        let original = observation(RemoteState::Succeeded, vec![]);
        let mut f = fixture(Ok(original.clone()));
        let count = f.count.clone();
        f.action = Some(Box::new(move || {
            let mut second = Application::open(&action_path).unwrap();
            let (mut b, _) = request();
            b.id = "second".into();
            second.reserve_call(&b).unwrap();
            if fail {
                let c = rusqlite::Connection::open(&action_path).unwrap();
                c.execute_batch("CREATE TRIGGER fixture_fail BEFORE UPDATE OF state ON aw_budget_intents WHEN NEW.state='UNKNOWN' BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
            }
        }));
        let mut e = Executor::new(f);
        let (b, r) = request();
        let Submission::Invoked {
            backend, recording, ..
        } = invoke(&mut a, &mut e, &b, &r)
        else {
            panic!("not invoked")
        };
        assert_eq!(backend.unwrap(), original);
        assert_eq!(recording.is_err(), fail);
        assert!(a.call_intent("second").unwrap().is_some());
        assert!(matches!(
            invoke(&mut a, &mut e, &b, &r),
            Submission::NotInvoked
        ));
        assert_eq!(count.get(), 1);
    }
}
#[test]
fn workflow_drives_traceable_local_step_and_blocks_cancel_or_failed_dependency() {
    for mode in 0..3 {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("db");
        let mut a = setup(&p);
        let mut driver = Application::open(&p).unwrap();
        a.create_workflow(
            "run",
            &[
                workflow::Step {
                    id: "prerequisite".into(),
                    dependencies: vec![],
                },
                workflow::Step {
                    id: "call".into(),
                    dependencies: vec!["prerequisite".into()],
                },
            ],
        )
        .unwrap();
        a.run_workflow_step("run", |_| {
            if mode == 1 {
                workflow::Outcome::Failure {
                    raw: "local prerequisite failed".into(),
                    evidence: None,
                }
            } else {
                workflow::Outcome::Success {
                    raw: "local prerequisite passed".into(),
                    evidence: None,
                }
            }
        })
        .unwrap();
        if mode == 2 {
            a.cancel_workflow("run").unwrap();
        }
        let f = fixture(Ok(observation(RemoteState::Unknown, vec![])));
        let count = f.count.clone();
        let mut e = Executor::new(f);
        let (b, r) = request();
        let ran = a
            .run_workflow_step("run", |id| {
                assert_eq!(id, b.id);
                assert!(matches!(
                    invoke(&mut driver, &mut e, &b, &r),
                    Submission::Invoked {
                        recording: Ok(()),
                        ..
                    }
                ));
                workflow::Outcome::Success {
                    raw: "local invocation recorded; remote unknown".into(),
                    evidence: Some("budget-call:call".into()),
                }
            })
            .unwrap();
        assert_eq!(ran, mode == 0);
        assert_eq!(count.get(), usize::from(mode == 0));
        if mode == 0 {
            assert!(a.workflow_steps("run").unwrap()[1].outcome.is_some());
            assert_eq!(
                a.call_intent("call").unwrap().unwrap().state,
                State::Unknown
            );
        } else {
            assert!(a.call_intent("call").unwrap().is_none());
        }
    }
}

#[test]
fn invalid_usage_and_zero_declarations_preserve_existing_guards() {
    for usage in [
        vec![Usage {
            unit: Unit::Token,
            amount: None,
            source: "bad\nsource".into(),
        }],
        vec![
            Usage {
                unit: Unit::Token,
                amount: Some(0),
                source: "fixture".into()
            };
            33
        ],
    ] {
        let d = tempfile::tempdir().unwrap();
        let mut a = setup(&d.path().join("db"));
        let mut e = Executor::new(fixture(Ok(observation(RemoteState::Succeeded, usage))));
        let (b, r) = request();
        assert!(matches!(
            invoke(&mut a, &mut e, &b, &r),
            Submission::Invoked {
                backend: Err(Error::InvalidUsage),
                recording: Ok(()),
                usage: None
            }
        ));
        assert_eq!(a.budget_balance("a").unwrap().reserved, 10);
        assert!(a.call_receipts(&b.id).unwrap().is_empty());
    }
    let d = tempfile::tempdir().unwrap();
    let mut a = setup(&d.path().join("db"));
    a.approve_budget(&Allowance {
        id: "zero".into(),
        unit: Unit::Token,
        capacity: 0,
        approval: Approval::ZeroAdditionalCost,
        source: "TEST_FIXTURE".into(),
    })
    .unwrap();
    let f = fixture(Ok(observation(RemoteState::Unknown, vec![])));
    let count = f.count.clone();
    let mut e = Executor::new(f);
    let (mut b, r) = request();
    b.account = "zero".into();
    assert!(
        a.submit_executor_local_test_fixture(&mut e, &b, &r)
            .is_err()
    );
    b.reservation = 0;
    for quote in [None, Some(1)] {
        b.quote = quote;
        assert!(
            a.submit_executor_local_test_fixture(&mut e, &b, &r)
                .is_err()
        );
    }
    assert_eq!(count.get(), 0);
    b.quote = Some(0);
    assert!(matches!(
        invoke(&mut a, &mut e, &b, &r),
        Submission::Invoked { .. }
    ));
    let mut changed = b.clone();
    changed.source = "changed".into();
    assert!(
        a.submit_executor_local_test_fixture(&mut e, &changed, &r)
            .is_err()
    );
    assert_eq!(count.get(), 1);
}
