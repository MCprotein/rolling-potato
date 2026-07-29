//! Raw terminal escape decoding into semantic editor actions.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Action {
    Left,
    Right,
    WordLeft,
    WordRight,
    Home,
    End,
    Up,
    Down,
    Delete,
    DeleteWord,
    Escape,
    PasteStart,
    ScrollUp,
    ScrollDown,
    Ignore,
}

pub(super) fn escape_sequence_complete(sequence: &[u8]) -> bool {
    sequence.len() == 2 && !matches!(sequence[1], b'[' | b'O')
        || sequence.len() >= 3
            && matches!(sequence[1], b'[' | b'O')
            && matches!(sequence.last(), Some(0x40..=0x7e))
        || sequence.len() >= 16
}

pub(super) fn decode_escape(sequence: &[u8]) -> Action {
    if sequence.starts_with(b"\x1b[<64;") && sequence.ends_with(b"M") {
        return Action::ScrollUp;
    }
    if sequence.starts_with(b"\x1b[<65;") && sequence.ends_with(b"M") {
        return Action::ScrollDown;
    }
    match sequence {
        b"\x1b[D" | b"\x1bOD" => Action::Left,
        b"\x1b[C" | b"\x1bOC" => Action::Right,
        b"\x1b[A" | b"\x1bOA" => Action::Up,
        b"\x1b[B" | b"\x1bOB" => Action::Down,
        b"\x1b[H" | b"\x1bOH" | b"\x1b[1~" | b"\x1b[7~" | b"\x1b[1;9D" => Action::Home,
        b"\x1b[F" | b"\x1bOF" | b"\x1b[4~" | b"\x1b[8~" | b"\x1b[1;9C" => Action::End,
        b"\x1bb" | b"\x1b[1;3D" | b"\x1b[1;5D" => Action::WordLeft,
        b"\x1bf" | b"\x1b[1;3C" | b"\x1b[1;5C" => Action::WordRight,
        b"\x1b\x7f" | b"\x1b[3;3~" => Action::DeleteWord,
        b"\x1b[3~" => Action::Delete,
        b"\x1b[5~" => Action::ScrollUp,
        b"\x1b[6~" => Action::ScrollDown,
        b"\x1b[200~" => Action::PasteStart,
        b"\x1b" => Action::Escape,
        _ => Action::Ignore,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incomplete_and_bounded_escape_sequences_are_distinguished() {
        assert!(!escape_sequence_complete(b"\x1b["));
        assert!(escape_sequence_complete(b"\x1b[A"));
        assert!(escape_sequence_complete(b"\x1bb"));
    }
}
