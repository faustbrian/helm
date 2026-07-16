use crate::control_plane::shared_infrastructure::SharedInstancePlan;
use crate::control_plane::workload::DedicatedProjectServicePlan;

/// Exact desired Engine plan selected for one restore resource kind.
pub(crate) enum ProjectRestoreTargetPlan {
    Shared(SharedInstancePlan),
    Dedicated(Box<DedicatedProjectServicePlan>),
}

impl ProjectRestoreTargetPlan {
    pub(crate) const fn shared(&self) -> Option<&SharedInstancePlan> {
        match self {
            Self::Shared(plan) => Some(plan),
            Self::Dedicated(_) => None,
        }
    }

    pub(crate) fn dedicated(&self) -> Option<&DedicatedProjectServicePlan> {
        match self {
            Self::Dedicated(plan) => Some(plan.as_ref()),
            Self::Shared(_) => None,
        }
    }
}
