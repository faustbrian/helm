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
