//! Responsibility: SQLite append-only exact-version feedback persistence.
//! Dependencies: Catalog Feedback capability, rusqlite and existing schema guard.
//! Callers: Application composition; invariant: checked transaction and frozen triggers decide binding atomically.
use super::{
    domain::{Decision, Feedback, FeedbackRecord, HistoryQuery, MetadataError},
    ports::Feedback as FeedbackPort,
};
use crate::library::{LibraryError, require_current};
use rusqlite::{Connection, TransactionBehavior, params};
pub(crate) struct SqliteFeedback<'a>(pub(crate) &'a mut Connection);
impl FeedbackPort for SqliteFeedback<'_> {
    fn append(&mut self, f: &Feedback) -> Result<FeedbackRecord, MetadataError> {
        append(self.0, f).map_err(Into::into)
    }
    fn history(&mut self, q: HistoryQuery<'_>) -> Result<Vec<FeedbackRecord>, MetadataError> {
        history(self.0, q).map_err(Into::into)
    }
}
pub(crate) fn append(conn: &mut Connection, f: &Feedback) -> Result<FeedbackRecord, LibraryError> {
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
