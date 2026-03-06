//! config types service hook module.
//!
//! Contains normalized hook configuration for service lifecycle phases.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookPhase {
    PostUp,
    PreDown,
    PostDown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookOnError {
    Fail,
    Warn,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HookRun {
    Exec { argv: Vec<String> },
    Script { path: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceHook {
    pub name: String,
    pub phase: HookPhase,
    pub run: HookRun,
    #[serde(default = "default_hook_on_error")]
    pub on_error: HookOnError,
    #[serde(default)]
    pub timeout_sec: Option<u64>,
}

const fn default_hook_on_error() -> HookOnError {
    HookOnError::Fail
}

#[cfg(test)]
mod tests {
    use super::{HookOnError, HookPhase, HookRun, ServiceHook};

    #[test]
    fn service_hook_defaults_error_to_fail() {
        let hook = ServiceHook {
            name: "hook".to_owned(),
            phase: HookPhase::PostUp,
            run: HookRun::Exec {
                argv: vec!["echo".to_owned()],
            },
            on_error: super::default_hook_on_error(),
            timeout_sec: None,
        };

        assert_eq!(hook.on_error, HookOnError::Fail);
        let exec = match hook.run {
            HookRun::Exec { argv } => argv,
            HookRun::Script { .. } => panic!("unexpected script variant"),
        };
        assert_eq!(exec[0], "echo");
    }
}
