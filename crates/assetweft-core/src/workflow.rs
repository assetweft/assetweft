//! Responsibility: bounded, persistent, caller-driven local workflow execution.
//! Dependencies: configured SQLite connection and the versioned product migration.
//! Callers: Application typed API. No SQL or executable commands cross that boundary.
//! Invariants: transactions never encompass callbacks; unresolved running steps block a run;
//! cancellation prevents future claims, preserving outcomes without asserting remote billing stopped.
//! Only caller-declared finite local work is scheduled; a failed prerequisite leaves dependents
//! pending. A process interruption or callback panic leaves the claim RUNNING, blocking the run
//! until the caller explicitly records an evidence-based outcome. This is not a remote executor,
//! does not cancel a running callback, and makes no power-loss or external billing guarantee.
use crate::library::{self, LibraryError};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::collections::{HashMap, HashSet};

/// Caller-declared stable step and its explicit prerequisite IDs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub id: String,
    pub dependencies: Vec<String>,
}
/// The driver's actual local outcome and optional evidence pointer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Success {
        raw: String,
        evidence: Option<String>,
    },
    Failure {
        raw: String,
        evidence: Option<String>,
    },
}
/// Running means claimed but unresolved and requires explicit reconciliation; it is never automatically rerun.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Pending,
    Running,
    Succeeded,
    Failed,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepRecord {
    pub id: String,
    pub state: State,
    pub outcome: Option<Outcome>,
}
fn invalid(s: &str) -> LibraryError {
    LibraryError::Database(s.to_owned())
}
fn id_ok(s: &str) -> bool {
    !s.is_empty() && s.len() <= 128 && !s.chars().any(char::is_control)
}

pub(crate) fn create(conn: &mut Connection, run: &str, steps: &[Step]) -> Result<(), LibraryError> {
    if !id_ok(run) || steps.is_empty() || steps.len() > 64 {
        return Err(invalid("invalid run id or step count (1..=64)"));
    }
    let mut ids = HashMap::new();
    for (i, s) in steps.iter().enumerate() {
        if !id_ok(&s.id) || s.dependencies.len() > 64 || ids.insert(s.id.as_str(), i).is_some() {
            return Err(invalid("invalid or duplicate step id / dependency count"));
        }
    }
    let mut indegree = vec![0usize; steps.len()];
    let mut edges = vec![Vec::new(); steps.len()];
    for (i, s) in steps.iter().enumerate() {
        let mut seen = HashSet::new();
        for d in &s.dependencies {
            let j = *ids
                .get(d.as_str())
                .ok_or_else(|| invalid(&format!("unknown dependency {d:?} for {:?}", s.id)))?;
            if !seen.insert(j) {
                return Err(invalid("duplicate dependency"));
            }
            indegree[i] += 1;
            edges[j].push(i);
        }
    }
    let mut queue: Vec<usize> = (0..steps.len()).filter(|i| indegree[*i] == 0).collect();
    let mut n = 0;
    while let Some(i) = queue.pop() {
        n += 1;
        for &j in &edges[i] {
            indegree[j] -= 1;
            if indegree[j] == 0 {
                queue.push(j);
            }
        }
    }
    if n != steps.len() {
        return Err(invalid("dependency cycle"));
    }
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
    if raw.len() > 1_048_576 || evidence.as_ref().is_some_and(|x| x.len() > 4096) {
        return Err(invalid("outcome exceeds bounds"));
    }
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
        let (state, outcome) = match (s.as_str(), raw) {
            ("PENDING", None) if evidence.is_none() => (State::Pending, None),
            ("RUNNING", None) if evidence.is_none() => (State::Running, None),
            ("SUCCEEDED", Some(raw)) => {
                (State::Succeeded, Some(Outcome::Success { raw, evidence }))
            }
            ("FAILED", Some(raw)) => (State::Failed, Some(Outcome::Failure { raw, evidence })),
            _ => return Err(invalid("invalid persisted step state/result")),
        };
        records.push(StepRecord { id, state, outcome });
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
