//! Responsibility: append and query exact-version user opinions, including latest selection.
//! Dependencies: Catalog pure validation and purpose-owned Feedback capability only.
//! Callers: Application composition; invariant: malformed inputs never reach persistence.
use super::{
    domain::{self, Feedback, FeedbackRecord, HistoryQuery, MetadataError},
    ports::Feedback as FeedbackPort,
};
pub(crate) fn append(
    port: &mut impl FeedbackPort,
    f: &Feedback,
) -> Result<FeedbackRecord, MetadataError> {
    domain::validate_feedback(f)?;
    port.append(f)
}
pub(crate) fn history(
    port: &mut impl FeedbackPort,
    q: HistoryQuery<'_>,
) -> Result<Vec<FeedbackRecord>, MetadataError> {
    domain::validate_history(q)?;
    port.history(q)
}
pub(crate) fn latest(
    port: &mut impl FeedbackPort,
    asset_id: &str,
    version_id: Option<&str>,
) -> Result<Option<FeedbackRecord>, MetadataError> {
    Ok(history(
        port,
        HistoryQuery {
            asset_id,
            version_id,
            before_sequence: None,
            limit: 1,
        },
    )?
    .into_iter()
    .next())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Default)]
    struct Memory {
        calls: Vec<String>,
    }
    impl FeedbackPort for Memory {
        fn append(&mut self, f: &Feedback) -> Result<FeedbackRecord, MetadataError> {
            self.calls
                .push(format!("append:{}:{}", f.asset_id, f.version_id));
            Ok(FeedbackRecord {
                sequence: 7,
                feedback: f.clone(),
            })
        }
        fn history(&mut self, q: HistoryQuery<'_>) -> Result<Vec<FeedbackRecord>, MetadataError> {
            self.calls.push(format!(
                "history:{}:{:?}:{:?}:{}",
                q.asset_id, q.version_id, q.before_sequence, q.limit
            ));
            Ok(vec![])
        }
    }
    #[test]
    fn validation_precedes_port_and_latest_is_a_bounded_exact_query() {
        let mut m = Memory::default();
        let mut f = Feedback {
            feedback_id: "f".into(),
            asset_id: "asset".into(),
            version_id: "v1".into(),
            decision: domain::Decision::Reject,
            reason: Some("reason".into()),
        };
        assert_eq!(append(&mut m, &f).unwrap().feedback, f);
        assert_eq!(latest(&mut m, "asset", Some("v1")).unwrap(), None);
        history(
            &mut m,
            HistoryQuery {
                asset_id: "asset",
                version_id: None,
                before_sequence: Some(7),
                limit: 2,
            },
        )
        .unwrap();
        assert_eq!(
            m.calls,
            [
                "append:asset:v1",
                "history:asset:Some(\"v1\"):None:1",
                "history:asset:None:Some(7):2"
            ]
        );
        m.calls.clear();
        f.feedback_id.clear();
        f.reason = Some(" ".into());
        assert_eq!(
            append(&mut m, &f),
            Err(MetadataError::InvalidInput("feedback identity"))
        );
        f.feedback_id = "f".into();
        assert_eq!(
            append(&mut m, &f),
            Err(MetadataError::InvalidInput("feedback reason"))
        );
        assert_eq!(
            history(
                &mut m,
                HistoryQuery {
                    asset_id: "",
                    version_id: None,
                    before_sequence: Some(0),
                    limit: 0
                }
            ),
            Err(MetadataError::InvalidInput("feedback query identity"))
        );
        assert_eq!(
            history(
                &mut m,
                HistoryQuery {
                    asset_id: "asset",
                    version_id: None,
                    before_sequence: Some(0),
                    limit: 1
                }
            ),
            Err(MetadataError::InvalidInput("feedback query bounds"))
        );
        assert!(m.calls.is_empty());
    }
}
