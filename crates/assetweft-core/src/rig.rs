//! Responsibility: legacy Rig compatibility exports.
//! Dependencies: Assurance public checks and values.
//! Callers: existing consumers and integration tests.
//! Invariants: same public Rig report and read errors.

pub use crate::assurance::checks::{
    inspect_rig_bytes as inspect_bytes, inspect_rig_file as inspect_file,
};
pub use crate::assurance::rig_domain::RigReport;
