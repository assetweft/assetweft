//! Responsibility: Rig numeric inspection capability.
//! Dependencies: plain Rig report.
//! Callers: Rig application and SDK adapter.
//! Invariants: SDK and byte readers remain outside core.

use super::rig_domain::RigReport;
pub(crate) trait RigNumeric {
    fn inspect(&self, bytes: &[u8], report: &mut RigReport);
}
