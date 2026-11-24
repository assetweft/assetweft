use super::{
    domain::{Decision, Feedback, FeedbackRecord, HistoryQuery, MetadataError},
    ports::Feedback as FeedbackPort,
};
use crate::library::{LibraryError, require_current};
use rusqlite::{Connection, Row, TransactionBehavior, params};
pub(crate) struct SqliteFeedback<'a>(pub(crate) &'a mut Connection);
impl FeedbackPort for SqliteFeedback<'_> {
    fn append(&mut self, f: &Feedback) -> Result<FeedbackRecord, MetadataError> {
        append(self.0, f).map_err(Into::into)
    }
    fn history(&mut self, q: HistoryQuery<'_>) -> Result<Vec<FeedbackRecord>, MetadataError> {
        history(self.0, q).map_err(Into::into)
    }
    fn by_id(
        &mut self,
        asset_id: &str,
        version_id: &str,
        feedback_id: &str,
    ) -> Result<Option<FeedbackRecord>, MetadataError> {
        by_id(self.0, asset_id, version_id, feedback_id).map_err(Into::into)
    }
}

fn by_id(
    conn: &mut Connection,
    asset_id: &str,
    version_id: &str,
    feedback_id: &str,
) -> Result<Option<FeedbackRecord>, LibraryError> {
    let tx = conn.transaction()?;
    require_current(&tx)?;
    let record = {
        let mut stmt = tx.prepare(
            "SELECT sequence,feedback_id,asset_id,version_id,decision,reason FROM aw_feedback WHERE feedback_id=?1 AND asset_id=?2 AND version_id=?3 LIMIT 1",
        )?;
        let mut rows = stmt.query(params![feedback_id, asset_id, version_id])?;
        rows.next()?.map(decode).transpose()?
    };
    tx.commit()?;
    Ok(record)
}

fn decode(row: &Row<'_>) -> Result<FeedbackRecord, LibraryError> {
    let decision = match row.get::<_, String>(4)?.as_str() {
        "ACCEPT" => Decision::Accept,
        "REJECT" => Decision::Reject,
        value => return Err(LibraryError::UnknownValue(value.to_owned())),
    };
    Ok(FeedbackRecord {
        sequence: row.get(0)?,
        feedback: Feedback {
            feedback_id: row.get(1)?,
            asset_id: row.get(2)?,
            version_id: row.get(3)?,
            decision,
            reason: row.get(5)?,
        },
    })
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
        let mut rows = stmt.query(params![
            q.asset_id,
            q.version_id,
            q.before_sequence,
            q.limit
        ])?;
        let mut result = Vec::new();
        while let Some(row) = rows.next()? {
            result.push(decode(row)?);
        }
        result
    };
    tx.commit()?;
    Ok(result)
}
