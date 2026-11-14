//! Responsibility: Consumption commands, queries and once-only local submission.
//! Dependencies: domain and atomic BudgetLedger only; compatibility facade assembles it.
//! Invariants: validate before persistence; callback follows durable claim, never retries.
use super::{domain::*, ports::BudgetLedger};
pub(crate) fn approve(ledger: &mut impl BudgetLedger, a: &Allowance) -> Result<()> {
    validate_allowance(a)?;
    ledger.approve(a)
}
pub(crate) fn balance(ledger: &mut impl super::ports::BudgetQueries, id: &str) -> Result<Balance> {
    ledger.balance(id)
}
pub(crate) fn reserve(ledger: &mut impl BudgetLedger, r: &Request) -> Result<Intent> {
    validate_request(r)?;
    ledger.reserve(r)
}
pub(crate) fn intent(
    ledger: &mut impl super::ports::BudgetQueries,
    id: &str,
) -> Result<Option<Intent>> {
    ledger.intent(id)
}
pub(crate) fn cancel(ledger: &mut impl BudgetLedger, id: &str) -> Result<()> {
    ledger.cancel(id)
}
pub(crate) fn reconcile(ledger: &mut impl BudgetLedger, id: &str, r: &Receipt) -> Result<()> {
    validate_receipt(r)?;
    ledger.reconcile(id, r)
}
pub(crate) fn receipts(
    ledger: &mut impl super::ports::BudgetQueries,
    id: &str,
) -> Result<Vec<Receipt>> {
    ledger.receipts(id)
}
pub(crate) fn submit_test_fixture(
    ledger: &mut impl BudgetLedger,
    id: &str,
    callback: impl FnOnce(),
) -> Result<bool> {
    if !ledger.claim(id)? {
        return Ok(false);
    }
    callback();
    ledger.unknown(id)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, rc::Rc};
    struct Ledger {
        events: Rc<RefCell<Vec<&'static str>>>,
        claim: Result<bool>,
        unknown: Result<()>,
    }
    impl BudgetLedger for Ledger {
        fn approve(&mut self, _: &Allowance) -> Result<()> {
            panic!("unexpected")
        }
        fn reserve(&mut self, _: &Request) -> Result<Intent> {
            panic!("validation must precede port")
        }
        fn claim(&mut self, _: &str) -> Result<bool> {
            self.events.borrow_mut().push("claim committed");
            self.claim.clone()
        }
        fn unknown(&mut self, _: &str) -> Result<()> {
            self.events.borrow_mut().push("unknown");
            self.unknown.clone()
        }
        fn cancel(&mut self, _: &str) -> Result<()> {
            panic!("unexpected")
        }
        fn reconcile(&mut self, _: &str, _: &Receipt) -> Result<()> {
            panic!("unexpected")
        }
    }
    #[test]
    fn callback_only_after_claim_and_record_failure_never_replays() {
        let events = Rc::new(RefCell::new(vec![]));
        let mut ledger = Ledger {
            events: events.clone(),
            claim: Ok(true),
            unknown: Err(BudgetError::Busy("fixture".into())),
        };
        assert_eq!(
            submit_test_fixture(&mut ledger, "id", || events.borrow_mut().push("callback")),
            Err(BudgetError::Busy("fixture".into()))
        );
        assert_eq!(*events.borrow(), ["claim committed", "callback", "unknown"]);
        for outcome in [Ok(false), Err(BudgetError::Uninitialized)] {
            events.borrow_mut().clear();
            ledger.claim = outcome.clone();
            assert_eq!(
                submit_test_fixture(&mut ledger, "id", || panic!("must not submit")),
                outcome
            );
            assert_eq!(*events.borrow(), ["claim committed"]);
        }
    }
    #[test]
    fn malformed_request_never_enters_ledger() {
        let mut ledger = Ledger {
            events: Rc::default(),
            claim: Ok(true),
            unknown: Ok(()),
        };
        let r = Request {
            id: String::new(),
            account: "a".into(),
            input_fingerprint: "d".into(),
            unit: Unit::Token,
            quote: None,
            reservation: 1,
            source: "local".into(),
        };
        assert_eq!(
            reserve(&mut ledger, &r),
            Err(BudgetError::InvalidInput("bounded text"))
        );
    }
}
