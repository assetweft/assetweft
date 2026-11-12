//! M2 local TEST_FIXTURE behavior; real file DBs, independent connections, no network.
//! Calls Application only except migration fault setup/inspection. Retain logs externally.
use assetweft_core::{application::Application, budget::*};
use std::sync::{
    Arc, Barrier,
    atomic::{AtomicUsize, Ordering},
};
use tempfile::tempdir;
fn setup(p: &std::path::Path) -> Application {
    let mut a = Application::open(p).unwrap();
    a.initialize_library().unwrap();
    a
}
fn allowance(unit: Unit, capacity: u64) -> Allowance {
    Allowance {
        id: "a".into(),
        unit,
        capacity,
        approval: Approval::ExplicitBounded,
        source: "TEST_FIXTURE caller declaration".into(),
    }
}
fn request(id: &str, n: u64) -> Request {
    Request {
        id: id.into(),
        account: "a".into(),
        input_fingerprint: "declared-input-v1".into(),
        unit: Unit::Token,
        quote: Some(n),
        reservation: n,
        source: "TEST_FIXTURE quote".into(),
    }
}
fn receipt(id: &str, resolution: Resolution) -> Receipt {
    Receipt {
        id: id.into(),
        source: "TEST_FIXTURE explicit query".into(),
        resolution,
        usage: vec![],
    }
}
#[test]
fn unknown_duplicate_guard_and_query_charges_once() {
    let d = tempdir().unwrap();
    let p = d.path().join("db");
    let mut a = setup(&p);
    a.approve_budget(&allowance(Unit::Token, 10)).unwrap();
    let r = request("lost", 7);
    a.reserve_call(&r).unwrap();
    let counter = AtomicUsize::new(0);
    assert!(
        a.submit_test_fixture("lost", || {
            counter.fetch_add(1, Ordering::SeqCst);
        })
        .unwrap()
    );
    drop(a);
    let mut a = setup(&p);
    assert_eq!(
        a.call_intent("lost").unwrap().unwrap().state,
        State::Unknown
    );
    assert_eq!(a.call_intent("lost").unwrap().unwrap().charge, None);
    assert_eq!(a.reserve_call(&r).unwrap().state, State::Unknown);
    assert!(
        !a.submit_test_fixture("lost", || {
            counter.fetch_add(1, Ordering::SeqCst);
        })
        .unwrap()
    );
    assert_eq!(counter.load(Ordering::SeqCst), 1);
    a.cancel_call("lost").unwrap();
    assert_eq!(a.budget_balance("a").unwrap().reserved, 7);
    assert!(a.reserve_call(&request("too-much", 4)).is_err());
    let q = receipt("query-1", Resolution::Confirmed { charge: 5 });
    a.reconcile_call("lost", &q).unwrap();
    a.reconcile_call("lost", &q).unwrap();
    assert!(
        a.reconcile_call(
            "lost",
            &receipt("query-2", Resolution::Confirmed { charge: 5 })
        )
        .is_err()
    );
    let b = a.budget_balance("a").unwrap();
    assert_eq!((b.reserved, b.spent), (0, 5));
    assert_eq!(a.call_receipts("lost").unwrap(), vec![q]);
    assert!(
        !a.submit_test_fixture("lost", || panic!("second purchase"))
            .unwrap()
    );
    let mut changed = r;
    changed.input_fingerprint = "different".into();
    assert!(a.reserve_call(&changed).is_err());
}
#[test]
fn interrupted_attempt_is_uncertain_and_workflow_stays_running() {
    let d = tempdir().unwrap();
    let p = d.path().join("db");
    let mut a = setup(&p);
    a.approve_budget(&allowance(Unit::Token, 10)).unwrap();
    a.reserve_call(&request("c", 10)).unwrap();
    a.create_workflow(
        "r",
        &[assetweft_core::workflow::Step {
            id: "s".into(),
            dependencies: vec![],
        }],
    )
    .unwrap();
    a.claim_workflow_step("r").unwrap();
    let count = AtomicUsize::new(0);
    let crashed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        a.submit_test_fixture("c", || {
            count.fetch_add(1, Ordering::SeqCst);
            panic!("accepted then interrupted");
        })
        .unwrap();
    }));
    assert!(crashed.is_err());
    drop(a);
    let mut a = setup(&p);
    assert_eq!(
        a.call_intent("c").unwrap().unwrap().state,
        State::Attempting
    );
    assert!(
        !a.submit_test_fixture("c", || {
            count.fetch_add(1, Ordering::SeqCst);
        })
        .unwrap()
    );
    assert_eq!(count.load(Ordering::SeqCst), 1);
    a.reconcile_call("c", &receipt("unknown", Resolution::Unresolved))
        .unwrap();
    assert_eq!(a.call_intent("c").unwrap().unwrap().state, State::Unknown);
    assert_eq!(
        a.workflow_steps("r").unwrap()[0].state,
        assetweft_core::workflow::State::Running
    );
    assert!(a.claim_workflow_step("r").unwrap().is_none());
    a.reconcile_call(
        "c",
        &receipt("no-charge", Resolution::AuthoritativeNoCharge),
    )
    .unwrap();
    assert_eq!(a.budget_balance("a").unwrap().reserved, 0);
    assert!(
        !a.submit_test_fixture("c", || panic!("released is not retryable"))
            .unwrap()
    );
}
#[test]
fn independent_connections_race_capacity_and_once_claim() {
    let d = tempdir().unwrap();
    let p = d.path().join("db");
    let mut a = setup(&p);
    a.approve_budget(&allowance(Unit::Token, 10)).unwrap();
    let barrier = Arc::new(Barrier::new(2));
    let mut threads = vec![];
    for id in ["x", "y"] {
        let p = p.clone();
        let b = barrier.clone();
        threads.push(std::thread::spawn(move || {
            let mut a = Application::open(&p).unwrap();
            b.wait();
            a.reserve_call(&request(id, 7)).is_ok()
        }));
    }
    assert_eq!(
        threads
            .into_iter()
            .map(|t| t.join().unwrap())
            .filter(|v| *v)
            .count(),
        1
    );
    assert_eq!(a.budget_balance("a").unwrap().reserved, 7);
    let id = if a.call_intent("x").unwrap().is_some() {
        "x"
    } else {
        "y"
    };
    let count = Arc::new(AtomicUsize::new(0));
    let mut threads = vec![];
    for _ in 0..2 {
        let p = p.clone();
        let b = barrier.clone();
        let count = count.clone();
        threads.push(std::thread::spawn(move || {
            let mut a = Application::open(&p).unwrap();
            b.wait();
            a.submit_test_fixture(id, || {
                count.fetch_add(1, Ordering::SeqCst);
            })
            .unwrap()
        }));
    }
    assert_eq!(
        threads
            .into_iter()
            .map(|t| usize::from(t.join().unwrap()))
            .sum::<usize>(),
        1
    );
    assert_eq!(count.load(Ordering::SeqCst), 1);
}
#[test]
fn units_unknown_zero_bounds_and_receipts_are_preserved() {
    let d = tempdir().unwrap();
    let mut a = setup(&d.path().join("db"));
    let mut zero = allowance(Unit::Token, 0);
    zero.approval = Approval::ZeroAdditionalCost;
    a.approve_budget(&zero).unwrap();
    let mut r = request("unknown", 0);
    r.quote = None;
    assert!(a.reserve_call(&r).is_err());
    r.quote = Some(0);
    a.reserve_call(&r).unwrap();
    a.submit_test_fixture(&r.id, || {}).unwrap();
    let mut q = receipt("q", Resolution::Unresolved);
    q.usage = vec![
        Usage {
            unit: Unit::Token,
            amount: Some(0),
            source: "count".into(),
        },
        Usage {
            unit: Unit::Task,
            amount: Some(1),
            source: "count".into(),
        },
        Usage {
            unit: Unit::Second,
            amount: None,
            source: "unavailable".into(),
        },
        Usage {
            unit: Unit::CurrencyMinor {
                currency: "USD".into(),
                decimals: 2,
            },
            amount: None,
            source: "unknown fee".into(),
        },
    ];
    a.reconcile_call(&r.id, &q).unwrap();
    assert_eq!(a.call_receipts(&r.id).unwrap(), vec![q.clone()]);
    q.usage[0].amount = Some(1);
    assert!(a.reconcile_call(&r.id, &q).is_err());
    r.id = "mixed".into();
    r.unit = Unit::Second;
    assert!(a.reserve_call(&r).is_err());
    let mut big = allowance(Unit::Task, u64::MAX);
    big.id = "big".into();
    assert!(a.approve_budget(&big).is_err());
    big.capacity = i64::MAX as u64;
    a.approve_budget(&big).unwrap();
    r.id = "max".into();
    r.account = "big".into();
    r.unit = Unit::Task;
    r.reservation = i64::MAX as u64;
    r.quote = Some(r.reservation);
    a.reserve_call(&r).unwrap();
    r.id = "overflow".into();
    r.reservation = 1;
    r.quote = Some(1);
    assert!(a.reserve_call(&r).is_err());
}
#[test]
fn callback_holds_no_transaction_and_cancel_does_not_release() {
    let d = tempdir().unwrap();
    let p = d.path().join("db");
    let mut a = setup(&p);
    a.approve_budget(&allowance(Unit::Token, 10)).unwrap();
    a.reserve_call(&request("c", 6)).unwrap();
    a.submit_test_fixture("c", || {
        let mut b = Application::open(&p).unwrap();
        b.reserve_call(&request("other", 4)).unwrap();
        b.cancel_call("c").unwrap();
    })
    .unwrap();
    assert_eq!(a.budget_balance("a").unwrap().reserved, 10);
    assert!(
        a.reconcile_call("c", &receipt("excess", Resolution::Confirmed { charge: 7 }))
            .is_err()
    );
    assert_eq!(a.budget_balance("a").unwrap().reserved, 10);
    a.cancel_call("other").unwrap();
    assert!(
        !a.submit_test_fixture("other", || panic!("cancelled"))
            .unwrap()
    );
}
fn v3(p: &std::path::Path) -> rusqlite::Connection {
    let c = rusqlite::Connection::open(p).unwrap();
    c.execute_batch("CREATE TABLE aw_schema_version(singleton INTEGER PRIMARY KEY,version INTEGER);INSERT INTO aw_schema_version VALUES(1,3);").unwrap();
    for sql in [
        include_str!("../src/library/001_initial.sql"),
        include_str!("../src/library/002_objects.sql"),
        include_str!("../src/library/003_workflow.sql"),
    ] {
        c.execute_batch(sql).unwrap();
    }
    c
}
#[test]
fn migration_004_rollback_and_faithful_v3_evolution() {
    let d = tempdir().unwrap();
    let p = d.path().join("db");
    let c = v3(&p);
    c.execute_batch("CREATE TABLE aw_budget_intents(keep TEXT);INSERT INTO aw_budget_intents VALUES('preserved');").unwrap();
    let mut a = Application::open(&p).unwrap();
    assert!(a.initialize_library().is_err());
    assert_eq!(
        c.query_row("SELECT version FROM aw_schema_version", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        3
    );
    assert_eq!(
        c.query_row(
            "SELECT count(*) FROM sqlite_schema WHERE name='aw_budget_accounts'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        c.query_row("SELECT keep FROM aw_budget_intents", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "preserved"
    );
    let p2 = d.path().join("evolve");
    let c2 = v3(&p2);
    c2.execute_batch("INSERT INTO aw_assets(asset_id) VALUES('legacy');")
        .unwrap();
    let mut a = setup(&p2);
    a.initialize_library().unwrap();
    assert_eq!(
        c2.query_row("SELECT version FROM aw_schema_version", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        5
    );
    assert_eq!(
        c2.query_row("SELECT asset_id FROM aw_assets", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "legacy"
    );
    c2.execute_batch("UPDATE aw_schema_version SET version=6")
        .unwrap();
    assert!(Application::open(&p2).is_err());
    assert!(a.approve_budget(&allowance(Unit::Token, 2)).is_err());
}
