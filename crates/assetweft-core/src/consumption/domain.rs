//! Responsibility: Consumption exact local declarations and pure budget decisions.
//! Dependencies: serde/std only; called by Consumption use cases and SQLite adapter.
//! Invariants: integer scale, immutable identities, UNKNOWN retains reservations; declarations are not rights proof.
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Unit {
    Token,
    Task,
    Second,
    CurrencyMinor { currency: String, decimals: u8 },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Approval {
    ZeroAdditionalCost,
    ExplicitBounded,
}
/// Immutable caller declaration, NOT authenticated user consent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Allowance {
    pub id: String,
    pub unit: Unit,
    pub capacity: u64,
    pub approval: Approval,
    pub source: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    pub id: String,
    pub account: String,
    pub input_fingerprint: String,
    pub unit: Unit,
    pub quote: Option<u64>,
    pub reservation: u64,
    pub source: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Preparing,
    Attempting,
    Unknown,
    Confirmed,
    Released,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Intent {
    pub request: Request,
    pub state: State,
    pub cancelled: bool,
    pub charge: Option<u64>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Balance {
    pub allowance: Allowance,
    pub reserved: u64,
    pub spent: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub unit: Unit,
    pub amount: Option<u64>,
    pub source: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Resolution {
    Unresolved,
    Confirmed { charge: u64 },
    AuthoritativeNoCharge,
}
/// Explicit query evidence, separate from submit. No job ID is inferred.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receipt {
    pub id: String,
    pub source: String,
    pub resolution: Resolution,
    pub usage: Vec<Usage>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BudgetError {
    Busy(String),
    Constraint(String),
    Io(String),
    Database(String),
    FutureSchema(i64),
    Uninitialized,
    InvalidInput(&'static str),
    UnknownValue(String),
}
pub(crate) type Result<T> = std::result::Result<T, BudgetError>;
fn invalid(s: &'static str) -> BudgetError {
    BudgetError::InvalidInput(s)
}
pub(crate) fn text(s: &str) -> Result<()> {
    if s.is_empty() || s.len() > 4096 || s.contains('\0') {
        Err(invalid("bounded text"))
    } else {
        Ok(())
    }
}
pub(crate) fn number(n: u64) -> Result<i64> {
    i64::try_from(n).map_err(|_| invalid("SQLite integer limit"))
}
fn unit(u: &Unit) -> Result<()> {
    if let Unit::CurrencyMinor { currency, decimals } = u
        && (currency.len() != 3
            || !currency.bytes().all(|x| x.is_ascii_uppercase())
            || *decimals > 9)
    {
        return Err(invalid("currency scale"));
    }
    Ok(())
}
pub(crate) fn validate_allowance(a: &Allowance) -> Result<()> {
    text(&a.id)?;
    text(&a.source)?;
    unit(&a.unit)?;
    let cap = number(a.capacity)?;
    if a.approval == Approval::ZeroAdditionalCost && cap != 0 {
        return Err(invalid("zero cost capacity"));
    }
    Ok(())
}
pub(crate) fn validate_request(r: &Request) -> Result<()> {
    text(&r.id)?;
    text(&r.account)?;
    text(&r.input_fingerprint)?;
    text(&r.source)?;
    unit(&r.unit)?;
    number(r.reservation)?;
    if let Some(q) = r.quote {
        number(q)?;
        if q > r.reservation {
            return Err(invalid("quote exceeds reservation"));
        }
    }
    Ok(())
}
pub(crate) fn validate_receipt(r: &Receipt) -> Result<()> {
    text(&r.id)?;
    text(&r.source)?;
    if r.usage.len() > 64 {
        return Err(invalid("usage count"));
    }
    for u in &r.usage {
        unit(&u.unit)?;
        text(&u.source)?;
        if let Some(n) = u.amount {
            number(n)?;
        }
    }
    Ok(())
}

pub(crate) fn reservation_policy(b: &Balance, r: &Request) -> Result<()> {
    if b.allowance.unit != r.unit
        || (b.allowance.approval == Approval::ZeroAdditionalCost && r.quote != Some(0))
    {
        return Err(invalid("unit or unknown possible charge"));
    }
    let remaining = b
        .allowance
        .capacity
        .checked_sub(b.reserved)
        .and_then(|n| n.checked_sub(b.spent));
    if remaining.is_none_or(|n| r.reservation > n) {
        return Err(invalid("approved capacity exhausted"));
    }
    Ok(())
}
pub(crate) fn same_request(old: &Intent, r: &Request) -> Result<()> {
    if old.request != *r {
        return Err(invalid("call identity conflict"));
    }
    Ok(())
}
pub(crate) fn same_descriptor(old: &str, new: &str, conflict: &'static str) -> Result<()> {
    if old != new {
        return Err(invalid(conflict));
    }
    Ok(())
}
pub(crate) fn release_on_cancel(i: &Intent) -> bool {
    i.state == State::Preparing
}
pub(crate) fn reconciliation(i: &Intent, r: &Receipt) -> Result<Resolution> {
    if !matches!(i.state, State::Attempting | State::Unknown) {
        return Err(invalid("call is not uncertain"));
    }
    Ok(r.resolution.clone())
}
pub(crate) fn settlement(i: &Intent, charge: u64) -> Result<i64> {
    let n = number(charge)?;
    if charge > i.request.reservation {
        return Err(invalid(
            "actual exceeds declared bound; reconciliation required",
        ));
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> Request {
        Request {
            id: "call".into(),
            account: "account".into(),
            input_fingerprint: "digest".into(),
            unit: Unit::Token,
            quote: None,
            reservation: 3,
            source: "local".into(),
        }
    }
    fn intent(state: State) -> Intent {
        Intent {
            request: request(),
            state,
            cancelled: false,
            charge: None,
        }
    }
    #[test]
    fn exact_capacity_unknown_quote_and_scale() {
        let mut b = Balance {
            allowance: Allowance {
                id: "account".into(),
                unit: Unit::Token,
                capacity: i64::MAX as u64,
                approval: Approval::ExplicitBounded,
                source: "local".into(),
            },
            reserved: i64::MAX as u64 - 4,
            spent: 1,
        };
        let mut r = request();
        assert_eq!(reservation_policy(&b, &r), Ok(()));
        r.reservation = 4;
        assert_eq!(
            reservation_policy(&b, &r),
            Err(invalid("approved capacity exhausted"))
        );
        r.reservation = 0;
        b.allowance.approval = Approval::ZeroAdditionalCost;
        assert_eq!(
            reservation_policy(&b, &r),
            Err(invalid("unit or unknown possible charge"))
        );
        r.quote = Some(0);
        assert_eq!(reservation_policy(&b, &r), Ok(()));
        assert_eq!(
            number(i64::MAX as u64 + 1),
            Err(invalid("SQLite integer limit"))
        );
        b.allowance.unit = Unit::CurrencyMinor {
            currency: "USD".into(),
            decimals: 2,
        };
        r.unit = Unit::CurrencyMinor {
            currency: "USD".into(),
            decimals: 3,
        };
        assert_eq!(
            reservation_policy(&b, &r),
            Err(invalid("unit or unknown possible charge"))
        );
    }
    #[test]
    fn cancellation_uncertainty_and_receipt_boundaries() {
        let receipt = Receipt {
            id: "receipt".into(),
            source: "query".into(),
            resolution: Resolution::Confirmed { charge: 2 },
            usage: vec![],
        };
        for state in [
            State::Preparing,
            State::Attempting,
            State::Unknown,
            State::Confirmed,
            State::Released,
        ] {
            let i = intent(state);
            assert_eq!(release_on_cancel(&i), state == State::Preparing);
            assert_eq!(
                reconciliation(&i, &receipt).is_ok(),
                matches!(state, State::Attempting | State::Unknown)
            );
        }
        let i = intent(State::Unknown);
        assert_eq!(settlement(&i, 3), Ok(3));
        assert_eq!(
            settlement(&i, 4),
            Err(invalid(
                "actual exceeds declared bound; reconciliation required"
            ))
        );
        let mut changed = i.request.clone();
        changed.input_fingerprint = "changed".into();
        assert_eq!(
            same_request(&i, &changed),
            Err(invalid("call identity conflict"))
        );
    }
}
