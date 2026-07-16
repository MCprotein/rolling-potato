use std::io::{self, IsTerminal};

pub(crate) fn attached() -> bool {
    io::stdin().is_terminal() && io::stdout().is_terminal()
}
