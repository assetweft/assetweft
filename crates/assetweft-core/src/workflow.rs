//! Responsibility: legacy workflow public type compatibility.
//! Dependencies: Production domain; callers: Application and legacy consumers.
//! Invariants: SAME public items; orchestration is composed by Application.
pub use crate::production::workflow_domain::{Outcome, State, Step, StepRecord};
