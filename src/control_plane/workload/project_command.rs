use std::fmt::{Debug, Formatter};

use super::{NodePackageManager, PhpTool};

/// One user-facing tool or declarative hook executed inside an application.
#[derive(Clone, Eq, PartialEq)]
pub(crate) enum ProjectCommand {
    Composer {
        arguments: Vec<String>,
    },
    NodePackageManager {
        package_manager: NodePackageManager,
        arguments: Vec<String>,
    },
    Bun {
        arguments: Vec<String>,
    },
    Artisan {
        arguments: Vec<String>,
    },
    Exec {
        arguments: Vec<String>,
    },
    PhpTool {
        tool: PhpTool,
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

impl Debug for ProjectCommand {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        let (kind, name, argument_count) = match self {
            Self::Composer { arguments } => ("composer", None, arguments.len()),
            Self::NodePackageManager { arguments, .. } => {
                ("node_package_manager", None, arguments.len())
            }
            Self::Bun { arguments } => ("bun", None, arguments.len()),
            Self::Artisan { arguments } => ("artisan", None, arguments.len()),
            Self::Exec { arguments } => ("exec", None, arguments.len()),
            Self::PhpTool { arguments, .. } => ("php_tool", None, arguments.len()),
            Self::Deno { arguments } => ("deno", None, arguments.len()),
            Self::Hook { name, arguments } => ("hook", Some(name), arguments.len()),
        };

        formatter
            .debug_struct("ProjectCommand")
            .field("kind", &kind)
            .field("name", &name)
            .field("argument_count", &argument_count)
            .finish()
    }
}

impl ProjectCommand {
    pub(super) fn into_parts(self) -> Result<(String, Vec<String>), String> {
        match self {
            Self::Composer { arguments } => Ok((
                "Composer".to_owned(),
                prefixed_arguments("composer", arguments),
            )),
            Self::NodePackageManager {
                package_manager,
                arguments,
            } => Ok((
                "Node package manager".to_owned(),
                prefixed_arguments(package_manager.executable(), arguments),
            )),
            Self::Bun { arguments } => Ok(("Bun".to_owned(), prefixed_arguments("bun", arguments))),
            Self::Artisan { arguments } => Ok((
                "Artisan".to_owned(),
                prefixed_arguments("php", prefixed_arguments("artisan", arguments)),
            )),
            Self::Exec { arguments } => {
                if arguments.first().is_none_or(String::is_empty) {
                    return Err("project exec command must not be empty".to_owned());
                }

                Ok(("Exec".to_owned(), arguments))
            }
            Self::PhpTool { tool, arguments } => Ok((
                "PHP tool".to_owned(),
                prefixed_arguments(tool.executable(), arguments),
            )),
            Self::Deno { arguments } => {
                Ok(("Deno".to_owned(), prefixed_arguments("deno", arguments)))
            }
            Self::Hook { name, arguments } => {
                if !valid_hook_name(&name) {
                    return Err(format!("project hook name '{name}' is invalid"));
                }
                if arguments.first().is_none_or(String::is_empty) {
                    return Err(format!("project hook '{name}' command must not be empty"));
                }

                Ok((format!("hook '{name}'"), arguments))
            }
        }
    }
}

fn prefixed_arguments(executable: &str, arguments: Vec<String>) -> Vec<String> {
    std::iter::once(executable.to_owned())
        .chain(arguments)
        .collect()
}

fn valid_hook_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 63
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && name
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        && name
            .as_bytes()
            .last()
            .is_some_and(u8::is_ascii_alphanumeric)
}
