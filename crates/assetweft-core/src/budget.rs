//! Responsibility: legacy budget public type compatibility.
//! Dependencies: Consumption domain; Application/executor consume identical types.
//! Invariants: SAME public items; Application composes Consumption use cases.
pub use crate::consumption::domain::{
    Allowance, Approval, Balance, Intent, Receipt, Request, Resolution, State, Unit, Usage,
};
