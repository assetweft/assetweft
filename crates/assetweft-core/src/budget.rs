//! Responsibility: M2 caller-declared local allowance and one-shot TEST_FIXTURE policy.
//! Dependencies: locked rusqlite/serde, library schema 004; called only by Application.
//! Invariants: no provider, authentication or money transfer; IMMEDIATE transactions end
//! before callbacks. ATTEMPTING is uncertain after interruption, never retryable. UNKNOWN
//! retains its reservation. Only an explicit authoritative receipt settles/reclaims it.
//! Units are integer counts in their declared scale, never converted. Shell can bypass this
//! policy. Request fingerprints are caller declarations, not provider idempotency guarantees.
use crate::library::{LibraryError, require_current};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

type Result<T> = std::result::Result<T, LibraryError>;
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
fn invalid(s: &'static str) -> LibraryError {
    LibraryError::InvalidInput(s)
}
fn text(s: &str) -> Result<()> {
    if s.is_empty() || s.len() > 4096 || s.contains('\0') {
        Err(invalid("bounded text"))
    } else {
        Ok(())
    }
}
fn number(n: u64) -> Result<i64> {
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
fn encode<T: Serialize>(x: &T) -> Result<String> {
    serde_json::to_string(x).map_err(|e| LibraryError::Database(e.to_string()))
}
fn decode<T: for<'a> Deserialize<'a>>(x: &str) -> Result<T> {
    serde_json::from_str(x).map_err(|e| LibraryError::Database(e.to_string()))
}
pub(crate) fn balance(c: &Connection, id: &str) -> Result<Balance> {
    require_current(c)?;
    let (d, r, s): (String, i64, i64) = c.query_row(
        "SELECT descriptor,reserved,spent FROM aw_budget_accounts WHERE id=?1",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    Ok(Balance {
        allowance: decode(&d)?,
        reserved: u64::try_from(r).map_err(|_| invalid("negative reserved"))?,
        spent: u64::try_from(s).map_err(|_| invalid("negative spent"))?,
    })
}
pub(crate) fn approve(c: &mut Connection, a: &Allowance) -> Result<()> {
    text(&a.id)?;
    text(&a.source)?;
    unit(&a.unit)?;
    let cap = number(a.capacity)?;
    if a.approval == Approval::ZeroAdditionalCost && cap != 0 {
        return Err(invalid("zero cost capacity"));
    }
    let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    require_current(&tx)?;
    let d = encode(a)?;
    let old: Option<String> = tx
        .query_row(
            "SELECT descriptor FROM aw_budget_accounts WHERE id=?1",
            [&a.id],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(old) = old {
        if old != d {
            return Err(invalid("allowance identity conflict"));
        }
    } else {
        tx.execute(
            "INSERT INTO aw_budget_accounts(id,descriptor,capacity) VALUES(?1,?2,?3)",
            params![a.id, d, cap],
        )?;
    }
    tx.commit()?;
    Ok(())
}
pub(crate) fn intent(c: &Connection, id: &str) -> Result<Option<Intent>> {
    require_current(c)?;
    let row: Option<(String, String, bool, Option<i64>)> = c
        .query_row(
            "SELECT descriptor,state,cancelled,charge FROM aw_budget_intents WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;
    row.map(|(d, s, cancelled, charge)| {
        Ok(Intent {
            request: decode(&d)?,
            state: match s.as_str() {
                "PREPARING" => State::Preparing,
                "ATTEMPTING" => State::Attempting,
                "UNKNOWN" => State::Unknown,
                "CONFIRMED" => State::Confirmed,
                "RELEASED" => State::Released,
                _ => return Err(LibraryError::UnknownValue(s)),
            },
            cancelled,
            charge: charge
                .map(u64::try_from)
                .transpose()
                .map_err(|_| invalid("negative charge"))?,
        })
    })
    .transpose()
}
pub(crate) fn reserve(c: &mut Connection, r: &Request) -> Result<Intent> {
    text(&r.id)?;
    text(&r.account)?;
    text(&r.input_fingerprint)?;
    text(&r.source)?;
    unit(&r.unit)?;
    let amount = number(r.reservation)?;
    if let Some(q) = r.quote {
        number(q)?;
        if q > r.reservation {
            return Err(invalid("quote exceeds reservation"));
        }
    }
    let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    require_current(&tx)?;
    if let Some(old) = intent(&tx, &r.id)? {
        if old.request != *r {
            return Err(invalid("call identity conflict"));
        }
        return Ok(old);
    }
    let b = balance(&tx, &r.account)?;
    if b.allowance.unit != r.unit
        || (b.allowance.approval == Approval::ZeroAdditionalCost && r.quote != Some(0))
    {
        return Err(invalid("unit or unknown possible charge"));
    }
    let n=tx.execute("UPDATE aw_budget_accounts SET reserved=reserved+?2 WHERE id=?1 AND ?2<=capacity-reserved-spent",params![r.account,amount])?;
    if n != 1 {
        return Err(invalid("approved capacity exhausted"));
    }
    tx.execute("INSERT INTO aw_budget_intents(id,account,descriptor,reservation,state) VALUES(?1,?2,?3,?4,'PREPARING')",params![r.id,r.account,encode(r)?,amount])?;
    let result = intent(&tx, &r.id)?.ok_or_else(|| invalid("missing intent"))?;
    tx.commit()?;
    Ok(result)
}
pub(crate) fn claim(c: &mut Connection, id: &str) -> Result<bool> {
    let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    require_current(&tx)?;
    let n=tx.execute("UPDATE aw_budget_intents SET state='ATTEMPTING' WHERE id=?1 AND state='PREPARING' AND cancelled=0",[id])?;
    tx.commit()?;
    Ok(n == 1)
}
pub(crate) fn unknown(c: &mut Connection, id: &str) -> Result<()> {
    let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    require_current(&tx)?;
    tx.execute(
        "UPDATE aw_budget_intents SET state='UNKNOWN' WHERE id=?1 AND state='ATTEMPTING'",
        [id],
    )?;
    tx.commit()?;
    Ok(())
}
pub(crate) fn cancel(c: &mut Connection, id: &str) -> Result<()> {
    let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    require_current(&tx)?;
    if tx.execute("UPDATE aw_budget_intents SET cancelled=1 WHERE id=?1", [id])? != 1 {
        return Err(invalid("unknown call"));
    }
    tx.commit()?;
    Ok(())
}
pub(crate) fn receipts(c: &Connection, id: &str) -> Result<Vec<Receipt>> {
    require_current(c)?;
    let mut st =
        c.prepare("SELECT descriptor FROM aw_budget_receipts WHERE call_id=?1 ORDER BY id")?;
    st.query_map([id], |r| r.get::<_, String>(0))?
        .map(|r| decode(&r?))
        .collect()
}
pub(crate) fn reconcile(c: &mut Connection, id: &str, r: &Receipt) -> Result<()> {
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
    let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    require_current(&tx)?;
    let i = intent(&tx, id)?.ok_or_else(|| invalid("unknown call"))?;
    let d = encode(r)?;
    let old: Option<String> = tx
        .query_row(
            "SELECT descriptor FROM aw_budget_receipts WHERE call_id=?1 AND id=?2",
            params![id, r.id],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(old) = old {
        if old != d {
            return Err(invalid("receipt identity conflict"));
        }
        return Ok(());
    }
    if !matches!(i.state, State::Attempting | State::Unknown) {
        return Err(invalid("call is not uncertain"));
    }
    match r.resolution {
        Resolution::Unresolved => {
            tx.execute(
                "UPDATE aw_budget_intents SET state='UNKNOWN' WHERE id=?1",
                [id],
            )?;
        }
        Resolution::Confirmed { charge } => {
            settle(&tx, &i, charge, "CONFIRMED")?;
        }
        Resolution::AuthoritativeNoCharge => {
            settle(&tx, &i, 0, "RELEASED")?;
        }
    }
    tx.execute(
        "INSERT INTO aw_budget_receipts(call_id,id,descriptor) VALUES(?1,?2,?3)",
        params![id, r.id, d],
    )?;
    tx.commit()?;
    Ok(())
}
fn settle(c: &Connection, i: &Intent, charge: u64, state: &str) -> Result<()> {
    let n = number(charge)?;
    if charge > i.request.reservation {
        return Err(invalid(
            "actual exceeds declared bound; reconciliation required",
        ));
    }
    c.execute(
        "UPDATE aw_budget_accounts SET reserved=reserved-?2,spent=spent+?3 WHERE id=?1",
        params![i.request.account, number(i.request.reservation)?, n],
    )?;
    c.execute(
        "UPDATE aw_budget_intents SET state=?2,charge=?3 WHERE id=?1",
        params![i.request.id, state, n],
    )?;
    Ok(())
}
