//! M1 version-bound feedback repository, called only through Application.
//! Uses rusqlite and migration 006; owns no filesystem or candidate acceptance logic.
//! Append-only decisions are user opinions, never file readiness, rights or current heads.
//! Caller IDs are durable identities; sequence is local commit order, not a sync ID.
use crate::library::{LibraryError, require_current};
use rusqlite::{Connection, TransactionBehavior, params};

pub const MAX_REASON_BYTES: usize = 2048;
pub const MAX_HISTORY_LIMIT: u16 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Accept,
    Reject,
}
impl Decision {
    fn as_str(self) -> &'static str {
        match self {
            Self::Accept => "ACCEPT",
            Self::Reject => "REJECT",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Feedback {
    pub feedback_id: String,
    pub asset_id: String,
    pub version_id: String,
    pub decision: Decision,
    /// None means no reason; otherwise 1–2048 UTF-8 bytes, nonblank, no NUL.
    pub reason: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedbackRecord {
    pub sequence: i64,
    pub feedback: Feedback,
}
/// Newest first, scoped to an asset and optionally an exact version.
/// Continue with before_sequence = last returned sequence; new appends do not shift pages.
#[derive(Debug, Clone, Copy)]
pub struct HistoryQuery<'a> {
    pub asset_id: &'a str,
    pub version_id: Option<&'a str>,
    pub before_sequence: Option<i64>,
    pub limit: u16,
}
fn identity(id: &str) -> bool {
    !id.trim().is_empty()
        && id.len() <= 256
        && !id.contains(['/', '\\', '\0'])
        && id != "."
        && id != ".."
}
pub(crate) fn append(conn: &mut Connection, f: &Feedback) -> Result<FeedbackRecord, LibraryError> {
    if !identity(&f.feedback_id) || !identity(&f.asset_id) || !identity(&f.version_id) {
        return Err(LibraryError::InvalidInput("feedback identity"));
    }
    if f.reason
        .as_ref()
        .is_some_and(|r| r.trim().is_empty() || r.len() > MAX_REASON_BYTES || r.contains('\0'))
    {
        return Err(LibraryError::InvalidInput("feedback reason"));
    }
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    require_current(&tx)?;
    tx.execute("INSERT INTO aw_feedback(feedback_id,asset_id,version_id,decision,reason) VALUES (?1,?2,?3,?4,?5)", params![f.feedback_id, f.asset_id, f.version_id, f.decision.as_str(), f.reason])?;
    let sequence = tx.last_insert_rowid();
    tx.commit()?;
    Ok(FeedbackRecord {
        sequence,
        feedback: f.clone(),
    })
}
pub(crate) fn history(
    conn: &mut Connection,
    q: HistoryQuery<'_>,
) -> Result<Vec<FeedbackRecord>, LibraryError> {
    if !identity(q.asset_id) || q.version_id.is_some_and(|id| !identity(id)) {
        return Err(LibraryError::InvalidInput("feedback query identity"));
    }
    if q.limit == 0 || q.limit > MAX_HISTORY_LIMIT || q.before_sequence.is_some_and(|s| s <= 0) {
        return Err(LibraryError::InvalidInput("feedback query bounds"));
    }
    let tx = conn.transaction()?;
    require_current(&tx)?;
    let result = {
        let mut stmt = tx.prepare("SELECT sequence,feedback_id,asset_id,version_id,decision,reason FROM aw_feedback WHERE asset_id=?1 AND (?2 IS NULL OR version_id=?2) AND (?3 IS NULL OR sequence<?3) ORDER BY sequence DESC LIMIT ?4")?;
        let rows = stmt.query_map(
            params![q.asset_id, q.version_id, q.before_sequence, q.limit],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, Option<String>>(5)?,
                ))
            },
        )?;
        let mut result = Vec::new();
        for row in rows {
            let (sequence, feedback_id, asset_id, version_id, value, reason) = row?;
            let decision = match value.as_str() {
                "ACCEPT" => Decision::Accept,
                "REJECT" => Decision::Reject,
                _ => return Err(LibraryError::UnknownValue(value)),
            };
            result.push(FeedbackRecord {
                sequence,
                feedback: Feedback {
                    feedback_id,
                    asset_id,
                    version_id,
                    decision,
                    reason,
                },
            });
        }
        result
    };
    tx.commit()?;
    Ok(result)
}
