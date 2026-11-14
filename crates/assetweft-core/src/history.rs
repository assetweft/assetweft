//! Responsibility: preserve legacy feedback type paths as identical Catalog values.
//! Dependencies: Catalog domain only; callers: existing public Application consumers.
//! Invariant: no duplicate types or persistence/business implementation.
pub use crate::catalog::domain::{
    Decision, Feedback, FeedbackRecord, HistoryQuery, MAX_HISTORY_LIMIT, MAX_REASON_BYTES,
};
