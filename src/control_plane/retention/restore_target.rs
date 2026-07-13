use super::RestoreTargetError;
use crate::control_plane::state::ResourceRecord;
use std::io::Read;

/// Resource-specific staging and atomic cutover for a verified backup.
pub(crate) trait RestoreTarget {
    /// Streams into an isolated target without changing the live resource.
    fn stage(
        &mut self,
        restore_id: &str,
        resource: &ResourceRecord,
        input: &mut dyn Read,
    ) -> Result<(), RestoreTargetError>;

    /// Performs target-native checks against the complete staged data.
    fn verify(
        &mut self,
        restore_id: &str,
        resource: &ResourceRecord,
    ) -> Result<(), RestoreTargetError>;

    /// Atomically makes verified staged data live.
    fn commit(
        &mut self,
        restore_id: &str,
        resource: &ResourceRecord,
    ) -> Result<(), RestoreTargetError>;

    /// Removes staged data or reverses a failed commit attempt.
    fn rollback(
        &mut self,
        restore_id: &str,
        resource: &ResourceRecord,
    ) -> Result<(), RestoreTargetError>;
}
