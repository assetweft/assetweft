//! Responsibility: concrete Consumption SQLite ledger, SQL mapping and transaction lifecycle.
//! Dependencies: rusqlite, library schema and core decisions; called only by concrete composition.
//! Invariants: original IMMEDIATE, conditional updates, rollback and exact descriptor identities.
use super::domain::*;
use crate::library::{LibraryError, require_current};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
type Result<T> = std::result::Result<T, LibraryError>;
fn invalid(s: &'static str) -> LibraryError {
    LibraryError::InvalidInput(s)
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
    let cap = number(a.capacity)?;
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
        same_descriptor(&old, &d, "allowance identity conflict")?;
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
    let amount = number(r.reservation)?;
    let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    require_current(&tx)?;
    if let Some(old) = intent(&tx, &r.id)? {
        same_request(&old, r)?;
        return Ok(old);
    }
    let b = balance(&tx, &r.account)?;
    reservation_policy(&b, r)?;
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
    let i = intent(&tx, id)?.ok_or_else(|| invalid("unknown call"))?;
    // Claim takes the same write lock: PREPARING proves no callback was submitted.
    // This is a local release, not a synthesized provider/query receipt. Terminal
    // states cannot release twice; ATTEMPTING/UNKNOWN still require reconciliation.
    if release_on_cancel(&i) {
        settle(&tx, &i, 0, "RELEASED")?;
    }
    tx.execute("UPDATE aw_budget_intents SET cancelled=1 WHERE id=?1", [id])?;
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
        same_descriptor(&old, &d, "receipt identity conflict")?;
        return Ok(());
    }
    match reconciliation(&i, r)? {
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
    let n = settlement(i, charge)?;
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
impl From<LibraryError> for BudgetError {
    fn from(e: LibraryError) -> Self {
        match e {
            LibraryError::Busy(v) => Self::Busy(v),
            LibraryError::Constraint(v) => Self::Constraint(v),
            LibraryError::Io(v) => Self::Io(v),
            LibraryError::Database(v) => Self::Database(v),
            LibraryError::FutureSchema(v) => Self::FutureSchema(v),
            LibraryError::InvalidInput(v) => Self::InvalidInput(v),
            LibraryError::UnknownValue(v) => Self::UnknownValue(v),
            LibraryError::Uninitialized => Self::Uninitialized,
        }
    }
}
impl From<BudgetError> for LibraryError {
    fn from(e: BudgetError) -> Self {
        match e {
            BudgetError::Busy(v) => Self::Busy(v),
            BudgetError::Constraint(v) => Self::Constraint(v),
            BudgetError::Io(v) => Self::Io(v),
            BudgetError::Database(v) => Self::Database(v),
            BudgetError::FutureSchema(v) => Self::FutureSchema(v),
            BudgetError::InvalidInput(v) => Self::InvalidInput(v),
            BudgetError::UnknownValue(v) => Self::UnknownValue(v),
            BudgetError::Uninitialized => Self::Uninitialized,
        }
    }
}
pub(crate) struct SqliteBudgetLedger<'a>(pub(crate) &'a mut Connection);
impl super::ports::BudgetLedger for SqliteBudgetLedger<'_> {
    fn approve(&mut self, a: &Allowance) -> super::domain::Result<()> {
        approve(self.0, a).map_err(Into::into)
    }
    fn reserve(&mut self, r: &Request) -> super::domain::Result<Intent> {
        reserve(self.0, r).map_err(Into::into)
    }
    fn claim(&mut self, id: &str) -> super::domain::Result<bool> {
        claim(self.0, id).map_err(Into::into)
    }
    fn unknown(&mut self, id: &str) -> super::domain::Result<()> {
        unknown(self.0, id).map_err(Into::into)
    }
    fn cancel(&mut self, id: &str) -> super::domain::Result<()> {
        cancel(self.0, id).map_err(Into::into)
    }
    fn reconcile(&mut self, id: &str, r: &Receipt) -> super::domain::Result<()> {
        reconcile(self.0, id, r).map_err(Into::into)
    }
}
pub(crate) struct BudgetQuery<'a>(pub(crate) &'a Connection);
impl super::ports::BudgetQueries for BudgetQuery<'_> {
    fn balance(&mut self, id: &str) -> super::domain::Result<Balance> {
        balance(self.0, id).map_err(Into::into)
    }
    fn intent(&mut self, id: &str) -> super::domain::Result<Option<Intent>> {
        intent(self.0, id).map_err(Into::into)
    }
    fn receipts(&mut self, id: &str) -> super::domain::Result<Vec<Receipt>> {
        receipts(self.0, id).map_err(Into::into)
    }
}
