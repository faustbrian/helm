use serde::{Deserialize, Serialize};

use super::{IpcNodePackageManager, IpcPhpTool};

/// A safe user-facing tool invocation transported without shell parsing.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
#[non_exhaustive]
pub(crate) enum IpcProjectCommand {
    Composer {
        arguments: Vec<String>,
    },
    NodePackageManager {
        package_manager: IpcNodePackageManager,
        arguments: Vec<String>,
    },
    Bun {
        arguments: Vec<String>,
    },
    Artisan {
        arguments: Vec<String>,
        #[serde(default)]
        browser: bool,
    },
    Exec {
        arguments: Vec<String>,
    },
    PhpTool {
        tool: IpcPhpTool,
        arguments: Vec<String>,
    },
    Deno {
        arguments: Vec<String>,
    },
    Hook {
        name: String,
        arguments: Vec<String>,
    },
}
