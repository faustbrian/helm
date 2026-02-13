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
