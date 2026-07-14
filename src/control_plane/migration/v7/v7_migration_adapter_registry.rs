use super::{V7MigrationAdapterExecutor, V7MigrationExecutionError};
use crate::control_plane::state::{V7MigrationAdapterCheckpoint, V7MigrationExecutionRecord};
use std::collections::BTreeMap;

/// Exact per-resource strategy registry for one immutable v7 execution plan.
#[derive(Default)]
pub(crate) struct V7MigrationAdapterRegistry<'adapter> {
    registrations: BTreeMap<String, V7MigrationAdapterRegistration<'adapter>>,
}

impl<'adapter> V7MigrationAdapterRegistry<'adapter> {
    pub(crate) fn register(
        &mut self,
        adapter_id: impl Into<String>,
        adapter_kind: impl Into<String>,
        executor: Box<dyn V7MigrationAdapterExecutor + 'adapter>,
    ) -> Result<(), String> {
        let adapter_id = adapter_id.into();
        let adapter_kind = adapter_kind.into();
        if adapter_id.is_empty()
            || adapter_kind.is_empty()
            || adapter_id.contains('\0')
            || adapter_kind.contains('\0')
        {
            return Err("v7 adapter registration requires non-empty identity and kind".to_owned());
        }
        if self
            .registrations
            .insert(
                adapter_id.clone(),
                V7MigrationAdapterRegistration {
                    adapter_kind,
                    executor,
                },
            )
            .is_some()
        {
            return Err(format!(
                "v7 migration adapter '{adapter_id}' is registered more than once"
            ));
        }

        Ok(())
    }

    pub(super) fn validate(
        &self,
        execution: &V7MigrationExecutionRecord,
    ) -> Result<(), V7MigrationExecutionError> {
        if self.registrations.len() != execution.checkpoints().len() {
            return Err(V7MigrationExecutionError::AdapterSetMismatch {
                expected: execution.checkpoints().len(),
                actual: self.registrations.len(),
            });
        }
        for checkpoint in execution.checkpoints() {
            self.registration(checkpoint)?;
        }

        Ok(())
    }

    pub(super) fn executor(
        &mut self,
        checkpoint: &V7MigrationAdapterCheckpoint,
    ) -> Result<&mut Box<dyn V7MigrationAdapterExecutor + 'adapter>, V7MigrationExecutionError>
    {
        let Some(registration) = self.registrations.get_mut(checkpoint.adapter_id()) else {
            return Err(missing(checkpoint));
        };
        if registration.adapter_kind != checkpoint.adapter_kind() {
            return Err(V7MigrationExecutionError::AdapterKindMismatch {
                adapter_id: checkpoint.adapter_id().to_owned(),
                expected: checkpoint.adapter_kind().to_owned(),
                actual: registration.adapter_kind.clone(),
            });
        }

        Ok(&mut registration.executor)
    }

    fn registration(
        &self,
        checkpoint: &V7MigrationAdapterCheckpoint,
    ) -> Result<&V7MigrationAdapterRegistration<'adapter>, V7MigrationExecutionError> {
        let registration = self
            .registrations
            .get(checkpoint.adapter_id())
            .ok_or_else(|| missing(checkpoint))?;
        if registration.adapter_kind != checkpoint.adapter_kind() {
            return Err(V7MigrationExecutionError::AdapterKindMismatch {
                adapter_id: checkpoint.adapter_id().to_owned(),
                expected: checkpoint.adapter_kind().to_owned(),
                actual: registration.adapter_kind.clone(),
            });
        }

        Ok(registration)
    }
}

struct V7MigrationAdapterRegistration<'adapter> {
    adapter_kind: String,
    executor: Box<dyn V7MigrationAdapterExecutor + 'adapter>,
}

fn missing(checkpoint: &V7MigrationAdapterCheckpoint) -> V7MigrationExecutionError {
    V7MigrationExecutionError::MissingAdapter {
        adapter_id: checkpoint.adapter_id().to_owned(),
        adapter_kind: checkpoint.adapter_kind().to_owned(),
    }
}
