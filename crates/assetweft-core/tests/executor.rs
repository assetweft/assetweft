//! Public protocol fixtures only; no provider, runtime admission or IO.
use assetweft_core::budget::{Unit, Usage};
use assetweft_core::executor::*;
use std::{cell::Cell, rc::Rc};
fn r(s: &str) -> Reference {
    Reference::new(s).unwrap()
}
fn declaration() -> Declaration {
    Declaration {
        backend: r("TEST_FIXTURE"),
        backend_version: r("1"),
        profile: r("fixture"),
        profile_version: r("1"),
        submit: true,
        query: true,
        download: true,
        cancel: CancelCapability::Confirmation,
    }
}
fn observation(state: RemoteState) -> Observation {
    Observation {
        state,
        job: None,
        usage: vec![Usage {
            unit: Unit::CurrencyMinor {
                currency: "USD".into(),
                decimals: 2,
            },
            amount: None,
            source: "TEST_FIXTURE".into(),
        }],
    }
}
struct Fixture {
    calls: Rc<Cell<usize>>,
    declaration: Declaration,
    result: Observation,
}
impl Backend for Fixture {
    fn declaration(&self) -> Declaration {
        self.declaration.clone()
    }
    fn submit(&mut self, _: &SubmitRequest) -> Result<Observation, Error> {
        self.calls.set(self.calls.get() + 1);
        Ok(self.result.clone())
    }
    fn query(&mut self, _: &Reference) -> Result<Observation, Error> {
        self.calls.set(self.calls.get() + 1);
        Ok(self.result.clone())
    }
    fn download(&mut self, a: &Reference) -> Result<Artifact, Error> {
        self.calls.set(self.calls.get() + 1);
        Ok(Artifact {
            reference: a.clone(),
        })
    }
    fn cancel(&mut self, _: &Reference) -> Result<CancelResult, Error> {
        self.calls.set(self.calls.get() + 1);
        Ok(CancelResult::Requested)
    }
}
// A distinct implementation uses the exact same consumer without another workflow.
struct Second(Fixture);
impl Backend for Second {
    fn declaration(&self) -> Declaration {
        self.0.declaration()
    }
    fn submit(&mut self, r: &SubmitRequest) -> Result<Observation, Error> {
        self.0.submit(r)
    }
    fn query(&mut self, r: &Reference) -> Result<Observation, Error> {
        self.0.query(r)
    }
    fn download(&mut self, r: &Reference) -> Result<Artifact, Error> {
        self.0.download(r)
    }
    fn cancel(&mut self, r: &Reference) -> Result<CancelResult, Error> {
        self.0.cancel(r)
    }
}
fn fixture(state: RemoteState) -> Fixture {
    Fixture {
        calls: Rc::new(Cell::new(0)),
        declaration: declaration(),
        result: observation(state),
    }
}
fn consume<B: Backend>(b: B, state: RemoteState) {
    let mut e = Executor::new(b);
    let result = e
        .submit(&SubmitRequest {
            correlation: r("call"),
            specification: r("spec"),
            credential: None,
        })
        .unwrap();
    assert_eq!(result.state, state);
    assert_eq!(result.job, None);
    assert_eq!(result.usage[0].amount, None);
    assert_eq!(e.query(&r("job")).unwrap().state, state);
    assert_eq!(e.cancel(&r("job")), Ok(CancelResult::Requested));
    assert_eq!(e.download(&r("artifact")).unwrap().reference, r("artifact"));
}
#[test]
fn interchangeable_and_unknown_is_not_replayed() {
    let f = fixture(RemoteState::Unknown);
    let count = f.calls.clone();
    consume(f, RemoteState::Unknown);
    assert_eq!(count.get(), 4);
    consume(
        Second(fixture(RemoteState::Accepted)),
        RemoteState::Accepted,
    );
}
#[test]
fn unsupported_before_backend_side_effect() {
    let mut f = fixture(RemoteState::Unknown);
    let count = f.calls.clone();
    f.declaration.submit = false;
    f.declaration.query = false;
    f.declaration.download = false;
    f.declaration.cancel = CancelCapability::Unsupported;
    let mut e = Executor::new(f);
    assert_eq!(
        e.submit(&SubmitRequest {
            correlation: r("c"),
            specification: r("s"),
            credential: None
        }),
        Err(Error::Unsupported(Operation::Submit))
    );
    assert_eq!(e.query(&r("j")), Err(Error::Unsupported(Operation::Query)));
    assert_eq!(
        e.download(&r("a")),
        Err(Error::Unsupported(Operation::Download))
    );
    assert_eq!(
        e.cancel(&r("j")),
        Err(Error::Unsupported(Operation::Cancel))
    );
    assert_eq!(count.get(), 0);
}
#[test]
fn bounded_refs_and_redacted_output() {
    for value in [
        "".to_owned(),
        "a".repeat(257),
        "with space".into(),
        "a\n".into(),
    ] {
        assert_eq!(Reference::new(&value), Err(Error::InvalidReference));
    }
    let sentinel = "SECRET_SENTINEL";
    let c = CredentialReference::SystemStore(r(sentinel));
    let req = SubmitRequest {
        correlation: r(sentinel),
        specification: r(sentinel),
        credential: Some(c.clone()),
    };
    for output in [
        format!("{c}"),
        format!("{c:?}"),
        format!("{req:?}"),
        format!("{}", r(sentinel)),
        format!("{:?}", Error::InvalidReference),
    ] {
        assert!(!output.contains(sentinel));
    }
    let mut o = observation(RemoteState::Unknown);
    o.usage[0].source = sentinel.into();
    assert!(!format!("{o:?}").contains(sentinel));
}
#[test]
fn usage_preserves_scale_unknown_zero_and_rejects_malformed() {
    let mut f = fixture(RemoteState::Succeeded);
    f.result.usage[0].amount = Some(0);
    let mut e = Executor::new(f);
    let o = e.query(&r("j")).unwrap();
    assert_eq!(o.usage[0].amount, Some(0));
    assert_eq!(
        o.usage[0].unit,
        Unit::CurrencyMinor {
            currency: "USD".into(),
            decimals: 2
        }
    );
    let mut f = fixture(RemoteState::Unknown);
    f.result.usage[0].source.clear();
    assert_eq!(Executor::new(f).query(&r("j")), Err(Error::InvalidUsage));
}
