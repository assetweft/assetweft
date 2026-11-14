//! Responsibility: SQLite adapter for Production persistent workflows.
//! Dependencies: configured SQLite/library schema and pure workflow policy.
//! Callers: legacy composition only; IMMEDIATE transactions own atomic transitions.
//! Invariants: original SQL/parameters retained; callback never enters this adapter.
use super::{
    workflow_domain::{self, Outcome, PolicyError, Step, StepRecord},
    workflow_ports::{WorkflowQuery, WorkflowRuns},
};
use crate::library::{self, LibraryError};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
fn invalid(s: &str) -> LibraryError {
    LibraryError::Database(s.to_owned())
}
impl From<PolicyError> for LibraryError {
    fn from(error: PolicyError) -> Self {
        Self::Database(error.0)
    }
}

pub(crate) struct SqliteWorkflowRuns<C>(pub C);
impl<C: std::ops::Deref<Target = Connection>> WorkflowQuery for SqliteWorkflowRuns<C> {
    type Error = LibraryError;
    fn read(&self, run: &str) -> Result<Vec<StepRecord>, LibraryError> {
        read(&self.0, run)
    }
    fn is_cancelled(&self, run: &str) -> Result<bool, LibraryError> {
        is_cancelled(&self.0, run)
    }
}
impl<C: std::ops::Deref<Target = Connection> + std::ops::DerefMut> WorkflowRuns
    for SqliteWorkflowRuns<C>
{
    fn create(&mut self, run: &str, steps: &[Step]) -> Result<(), LibraryError> {
        create(&mut self.0, run, steps)
    }
    fn claim(&mut self, run: &str) -> Result<Option<String>, LibraryError> {
        claim(&mut self.0, run)
    }
    fn finish(&mut self, run: &str, id: &str, outcome: &Outcome) -> Result<(), LibraryError> {
        finish(&mut self.0, run, id, outcome)
    }
    fn cancel(&mut self, run: &str) -> Result<(), LibraryError> {
        cancel(&mut self.0, run)
    }
}
pub(crate) fn create(conn: &mut Connection, run: &str, steps: &[Step]) -> Result<(), LibraryError> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    library::require_current(&tx)?;
    tx.execute("INSERT INTO aw_workflow_runs VALUES (?1,0)", [run])?;
    for (i, s) in steps.iter().enumerate() {
        tx.execute(
            "INSERT INTO aw_workflow_steps VALUES (?1,?2,?3,'PENDING',NULL,NULL)",
            params![run, s.id, i as i64],
        )?;
    }
    for s in steps {
        for d in &s.dependencies {
            tx.execute(
                "INSERT INTO aw_workflow_dependencies VALUES (?1,?2,?3)",
                params![run, s.id, d],
            )?;
        }
    }
    tx.commit()?;
    Ok(())
}
pub(crate) fn claim(conn: &mut Connection, run: &str) -> Result<Option<String>, LibraryError> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    library::require_current(&tx)?;
    let cancelled: i64 = tx
        .query_row(
            "SELECT cancelled FROM aw_workflow_runs WHERE id=?1",
            [run],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| invalid("unknown run"))?;
    let id: Option<String> = if cancelled == 0 {
        tx.query_row("SELECT s.id FROM aw_workflow_steps s WHERE s.run_id=?1 AND s.state='PENDING' AND NOT EXISTS (SELECT 1 FROM aw_workflow_steps active WHERE active.run_id=s.run_id AND active.state='RUNNING') AND NOT EXISTS (SELECT 1 FROM aw_workflow_dependencies d JOIN aw_workflow_steps p ON p.run_id=d.run_id AND p.id=d.dependency_id WHERE d.run_id=s.run_id AND d.step_id=s.id AND p.state != 'SUCCEEDED') ORDER BY s.ordinal LIMIT 1",[run],|r|r.get(0)).optional()?
    } else {
        None
    };
    if cancelled == 0 {
        let records = read(&tx, run)?;
        let mut stmt = tx.prepare(
            "SELECT step_id,dependency_id FROM aw_workflow_dependencies WHERE run_id=?1",
        )?;
        let dependencies = stmt
            .query_map([run], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<Vec<(String, String)>, _>>()?;
        // SQL projects a candidate; the core owns permission to transition it.
        if workflow_domain::ready_step(false, &records, &dependencies) != id.as_deref() {
            return Err(invalid("invalid persisted step state/result"));
        }
    }
    if let Some(ref id) = id {
        tx.execute("UPDATE aw_workflow_steps SET state='RUNNING' WHERE run_id=?1 AND id=?2 AND state='PENDING'",params![run,id])?;
    }
    tx.commit()?;
    Ok(id)
}
pub(crate) fn finish(
    conn: &mut Connection,
    run: &str,
    id: &str,
    outcome: &Outcome,
) -> Result<(), LibraryError> {
    let (state, raw, evidence) = match outcome {
        Outcome::Success { raw, evidence } => ("SUCCEEDED", raw, evidence),
        Outcome::Failure { raw, evidence } => ("FAILED", raw, evidence),
    };
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    library::require_current(&tx)?;
    let changed=tx.execute("UPDATE aw_workflow_steps SET state=?3,raw=?4,evidence=?5 WHERE run_id=?1 AND id=?2 AND state='RUNNING'",params![run,id,state,raw,evidence])?;
    if changed != 1 {
        return Err(invalid("step is not running"));
    }
    tx.commit()?;
    Ok(())
}
pub(crate) fn cancel(conn: &mut Connection, run: &str) -> Result<(), LibraryError> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    library::require_current(&tx)?;
    if tx.execute("UPDATE aw_workflow_runs SET cancelled=1 WHERE id=?1", [run])? != 1 {
        return Err(invalid("unknown run"));
    }
    tx.commit()?;
    Ok(())
}
pub(crate) fn read(conn: &Connection, run: &str) -> Result<Vec<StepRecord>, LibraryError> {
    library::require_current(conn)?;
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM aw_workflow_runs WHERE id=?1)",
        [run],
        |r| r.get(0),
    )?;
    if !exists {
        return Err(invalid("unknown run"));
    }
    let mut stmt = conn.prepare(
        "SELECT id,state,raw,evidence FROM aw_workflow_steps WHERE run_id=?1 ORDER BY ordinal",
    )?;
    let rows = stmt.query_map([run], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, Option<String>>(3)?,
        ))
    })?;
    let mut records = Vec::new();
    for row in rows {
        let (id, s, raw, evidence) = row?;
        let record = workflow_domain::decode_record(id, &s, raw, evidence)?;
        records.push(record);
    }
    Ok(records)
}

/// Reads the durable cancellation request; it says nothing about remote execution or billing.
pub(crate) fn is_cancelled(conn: &Connection, run: &str) -> Result<bool, LibraryError> {
    library::require_current(conn)?;
    let value: i64 = conn
        .query_row(
            "SELECT cancelled FROM aw_workflow_runs WHERE id=?1",
            [run],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| invalid("unknown run"))?;
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(invalid("invalid cancellation state")),
    }
}
