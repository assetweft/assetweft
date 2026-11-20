//! Exercises both public budget writers against a fresh library. SQLite hooks on
//! the two test connections synchronize the first commit and a real lock conflict.
use super::Application;
use crate::{budget::*, library::LibraryError};
use rusqlite::trace::{TraceEvent, TraceEventCodes};
use std::{sync::mpsc::channel, thread, time::Duration};

const WAIT: Duration = Duration::from_secs(5);

fn run_writer(
    app: &mut Application,
    cancel: bool,
    calls: &mut usize,
) -> Result<bool, LibraryError> {
    if cancel {
        app.cancel_call("race").map(|()| false)
    } else {
        app.submit_test_fixture("race", || *calls += 1)
    }
}

#[test]
fn overlapping_claim_and_cancel() {
    for cancel_first in [true, false] {
        for veto in [false, true] {
            check_overlap(cancel_first, veto);
        }
    }
}

fn check_overlap(cancel_first: bool, veto: bool) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let mut first = Application::open(&path).unwrap();
    first.initialize_library().unwrap();
    let approved = Allowance {
        id: "a".into(),
        unit: Unit::Token,
        capacity: 10,
        approval: Approval::ExplicitBounded,
        source: "local test fixture".into(),
    };
    let request = |id: &str, n| Request {
        id: id.into(),
        account: "a".into(),
        input_fingerprint: "fixture".into(),
        unit: Unit::Token,
        quote: Some(n),
        reservation: n,
        source: "local test fixture".into(),
    };
    first.approve_budget(&approved).unwrap();
    first.reserve_call(&request("race", 10)).unwrap();
    let mut second = Application::open(&path).unwrap();

    let (held, start) = channel();
    let (collision, observed) = channel();
    let (finished, retry) = channel();
    // Only gate the first commit. A successful claim also records UNKNOWN.
    let mut held = Some(held);
    first
        .connection
        .commit_hook(Some(move || {
            let Some(signal) = held.take() else {
                return false;
            };
            // On disconnect/timeout abort. The controller verifies the observation
            // independently because SQLite callbacks cannot propagate a Rust panic.
            signal.send(()).is_err() || observed.recv_timeout(WAIT).is_err() || veto
        }))
        .unwrap();

    let (sql, statements) = channel();
    second
        .connection
        .trace_v2(
            TraceEventCodes::SQLITE_TRACE_STMT,
            Some(move |event: TraceEvent<'_>| {
                if let TraceEvent::Stmt(_, statement) = event {
                    let _ = sql.send(statement.to_owned());
                }
            }),
        )
        .unwrap();
    let (witness, witnessed) = channel();
    second
        .connection
        .busy_handler(Some(move |attempt| {
            attempt == 0
                && witness.send(()).is_ok()
                && collision.send(()).is_ok()
                && retry.recv_timeout(WAIT).is_ok()
        }))
        .unwrap();
    let worker = thread::spawn(move || {
        start.recv_timeout(WAIT).expect("first writer at commit");
        let mut calls = 0;
        let result = run_writer(&mut second, !cancel_first, &mut calls);
        (result, calls)
    });
    let mut calls = 0;
    let first_result = run_writer(&mut first, cancel_first, &mut calls);
    witnessed
        .recv_timeout(WAIT)
        .expect("SQLite lock conflict before commit");
    assert_eq!(
        statements.try_iter().last().as_deref(),
        Some("BEGIN IMMEDIATE")
    );
    finished.send(()).unwrap();
    let (second_result, second_calls) = worker.join().unwrap();
    if veto {
        assert!(
            matches!(first_result, Err(LibraryError::Constraint(_))),
            "{first_result:?}"
        );
    } else {
        assert_eq!(first_result.unwrap(), !cancel_first);
    }
    assert_eq!(second_result.unwrap(), cancel_first && veto);

    let submitted = cancel_first == veto;
    assert_eq!(calls + second_calls, usize::from(submitted));
    let mut app = Application::open(&path).unwrap();
    let intent = app.call_intent("race").unwrap().unwrap();
    assert_eq!(intent.cancelled, !cancel_first || !veto);
    assert!(app.call_receipts("race").unwrap().is_empty());
    app.cancel_call("race").unwrap();
    assert!(
        !app.submit_test_fixture("race", || panic!("duplicate callback"))
            .unwrap()
    );
    let balance = app.budget_balance("a").unwrap();
    assert_eq!(balance.allowance, approved);
    let spent = if submitted {
        assert_eq!((intent.state, intent.charge), (State::Unknown, None));
        assert_eq!((balance.reserved, balance.spent), (10, 0));
        assert_eq!(
            app.reserve_call(&request("overflow", 1)),
            Err(LibraryError::InvalidInput("approved capacity exhausted"))
        );
        let receipt = Receipt {
            id: "final".into(),
            source: "local test fixture".into(),
            resolution: Resolution::Confirmed { charge: 4 },
            usage: vec![],
        };
        app.reconcile_call("race", &receipt).unwrap();
        app.reconcile_call("race", &receipt).unwrap();
        assert_eq!(app.call_receipts("race").unwrap(), vec![receipt]);
        4
    } else {
        assert_eq!((intent.state, intent.charge), (State::Released, Some(0)));
        0
    };
    let balance = app.budget_balance("a").unwrap();
    assert_eq!((balance.reserved, balance.spent), (0, spent));
    app.reserve_call(&request("reuse", approved.capacity - spent))
        .unwrap();
    assert_eq!(
        app.reserve_call(&request("overflow", 1)),
        Err(LibraryError::InvalidInput("approved capacity exhausted"))
    );
}
