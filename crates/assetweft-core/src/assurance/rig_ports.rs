use super::rig_domain::RigReport;
pub(crate) trait RigNumeric {
    fn inspect(&self, bytes: &[u8], report: &mut RigReport);
}
