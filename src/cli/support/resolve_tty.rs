//! cli support resolve tty module.
//!
//! Contains cli support resolve tty logic used by Helm command workflows.

/// Resolves tty using configured inputs and runtime state.
pub(crate) fn resolve_tty(tty: bool, no_tty: bool) -> bool {
    use std::io::IsTerminal;

    if tty {
        return true;
    }
    if no_tty {
        return false;
    }

    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}
