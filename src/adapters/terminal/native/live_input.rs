use std::io::{self, Read};

use super::{TerminalFault, TerminalSuggestion};

mod editor;
mod render;

use editor::Editor;
use render::BracketedPasteGuard;

const MAX_INPUT_BYTES: usize = 8 * 1024;
const MAX_PALETTE_ROWS: usize = 6;
const PASTE_END: &[u8] = b"\x1b[201~";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
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
    Ignore,
}

pub(super) fn read(
    suggestions: &[TerminalSuggestion],
    terminal_width: usize,
    base_frame: &str,
) -> Result<Option<String>, TerminalFault> {
    let _paste_guard = BracketedPasteGuard::start()?;
    let mut editor = Editor::default();
    let mut escape = Vec::new();
    let mut utf8 = Vec::new();
    let mut paste = None::<Vec<u8>>;
    let mut stdin = io::stdin().lock();
    redraw(&editor, suggestions, terminal_width, base_frame)?;

    loop {
        let mut byte = [0_u8; 1];
        let bytes = stdin.read(&mut byte).map_err(|_| TerminalFault::LineRead)?;
        if bytes == 0 {
            if escape == [0x1b] {
                apply_action(&mut editor, Action::Escape, suggestions);
                escape.clear();
                redraw(&editor, suggestions, terminal_width, base_frame)?;
            }
            continue;
        }
        let byte = byte[0];
        if let Some(pasted) = paste.as_mut() {
            if pasted.len() < MAX_INPUT_BYTES + PASTE_END.len() {
                pasted.push(byte);
            }
            if pasted.ends_with(PASTE_END) {
                pasted.truncate(pasted.len() - PASTE_END.len());
                let normalized = normalize_paste(pasted)?;
                if editor.text().len() + normalized.len() <= MAX_INPUT_BYTES {
                    editor.insert(&normalized);
                }
                paste = None;
                redraw(&editor, suggestions, terminal_width, base_frame)?;
            }
            continue;
        }
        if !escape.is_empty() || byte == 0x1b {
            escape.push(byte);
            if escape_sequence_complete(&escape) {
                let action = decode_escape(&escape);
                escape.clear();
                if action == Action::PasteStart {
                    paste = Some(Vec::new());
                } else {
                    apply_action(&mut editor, action, suggestions);
                    redraw(&editor, suggestions, terminal_width, base_frame)?;
                }
            }
            continue;
        }
        match byte {
            b'\n' | b'\r' => {
                if accept_suggestion(&mut editor, suggestions) {
                    redraw(&editor, suggestions, terminal_width, base_frame)?;
                    continue;
                }
                redraw(&editor, &[], terminal_width, base_frame)?;
                return Ok(Some(editor.into_text()));
            }
            0x03 => return Ok(None),
            0x04 if editor.text().is_empty() => return Ok(None),
            0x04 => editor.delete(),
            0x01 => editor.home(),
            0x02 => editor.left(),
            0x05 => editor.end(),
            0x06 => editor.right(),
            0x08 | 0x7f => editor.backspace(),
            0x09 => {
                accept_suggestion(&mut editor, suggestions);
            }
            0x0b => editor.delete_to_end(),
            0x0e => apply_action(&mut editor, Action::Down, suggestions),
            0x10 => apply_action(&mut editor, Action::Up, suggestions),
            0x15 => editor.delete_to_start(),
            0x17 => editor.delete_word_back(),
            byte if !byte.is_ascii_control() && editor.text().len() < MAX_INPUT_BYTES => {
                utf8.push(byte);
                match std::str::from_utf8(&utf8) {
                    Ok(value) => {
                        editor.insert(value);
                        utf8.clear();
                    }
                    Err(error) if error.error_len().is_none() => continue,
                    Err(_) => return Err(TerminalFault::LineRead),
                }
            }
            _ => continue,
        }
        redraw(&editor, suggestions, terminal_width, base_frame)?;
    }
}

fn apply_action(editor: &mut Editor, action: Action, suggestions: &[TerminalSuggestion]) {
    let count = visible_suggestions(editor, suggestions).len();
    match action {
        Action::Left => editor.left(),
        Action::Right => editor.right(),
        Action::WordLeft => editor.word_left(),
        Action::WordRight => editor.word_right(),
        Action::Home => editor.home(),
        Action::End => editor.end(),
        Action::Up => editor.previous_suggestion(count),
        Action::Down => editor.next_suggestion(count),
        Action::Delete => editor.delete(),
        Action::DeleteWord => editor.delete_word_back(),
        Action::Escape => editor.escape(),
        Action::PasteStart | Action::Ignore => {}
    }
}

fn accept_suggestion(editor: &mut Editor, suggestions: &[TerminalSuggestion]) -> bool {
    let matches = visible_suggestions(editor, suggestions);
    let Some(entry) = matches.get(editor.selected.min(matches.len().saturating_sub(1))) else {
        return false;
    };
    let token = entry
        .command
        .split_whitespace()
        .next()
        .unwrap_or(entry.command);
    let incomplete = editor.text() != token || entry.command.contains('<');
    if incomplete {
        editor.replace_with_command(entry.command);
    }
    incomplete
}

fn escape_sequence_complete(sequence: &[u8]) -> bool {
    sequence.len() == 2 && !matches!(sequence[1], b'[' | b'O')
        || sequence.len() >= 3
            && matches!(sequence[1], b'[' | b'O')
            && matches!(sequence.last(), Some(0x40..=0x7e))
        || sequence.len() >= 16
}

fn decode_escape(sequence: &[u8]) -> Action {
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
        b"\x1b[200~" => Action::PasteStart,
        b"\x1b" => Action::Escape,
        _ => Action::Ignore,
    }
}

fn normalize_paste(bytes: &[u8]) -> Result<String, TerminalFault> {
    let value = std::str::from_utf8(bytes).map_err(|_| TerminalFault::LineRead)?;
    Ok(value
        .chars()
        .map(|ch| {
            if matches!(ch, '\r' | '\n' | '\t') {
                ' '
            } else {
                ch
            }
        })
        .collect())
}

fn matching_suggestions<'a>(
    input: &str,
    suggestions: &'a [TerminalSuggestion],
) -> Vec<&'a TerminalSuggestion> {
    if !input.starts_with('/') || input.chars().any(char::is_whitespace) {
        return Vec::new();
    }
    suggestions
        .iter()
        .filter(|entry| {
            entry
                .command
                .split_whitespace()
                .next()
                .is_some_and(|command| command.starts_with(input))
        })
        .take(MAX_PALETTE_ROWS)
        .collect()
}

fn visible_suggestions<'a>(
    editor: &Editor,
    suggestions: &'a [TerminalSuggestion],
) -> Vec<&'a TerminalSuggestion> {
    if editor.palette_hidden {
        Vec::new()
    } else {
        matching_suggestions(editor.text(), suggestions)
    }
}

fn redraw(
    editor: &Editor,
    suggestions: &[TerminalSuggestion],
    terminal_width: usize,
    base_frame: &str,
) -> Result<(), TerminalFault> {
    render::redraw(editor, suggestions, terminal_width, base_frame)
}

#[cfg(test)]
fn pop_last_utf8_char(input: &mut Vec<u8>) {
    let Some(last) = input.pop() else { return };
    if last & 0b1100_0000 == 0b1000_0000 {
        while matches!(input.last(), Some(byte) if byte & 0b1100_0000 == 0b1000_0000) {
            input.pop();
        }
        input.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUGGESTIONS: &[TerminalSuggestion] = &[
        TerminalSuggestion {
            command: "/model [id]",
            description: "모델 변경",
        },
        TerminalSuggestion {
            command: "/search <질문>",
            description: "웹 검색",
        },
        TerminalSuggestion {
            command: "/help",
            description: "도움말",
        },
    ];

    #[test]
    fn filters_command_tokens_and_accepts_required_arguments() {
        assert_eq!(matching_suggestions("/", SUGGESTIONS).len(), 3);
        assert_eq!(
            matching_suggestions("/mo", SUGGESTIONS)[0].command,
            "/model [id]"
        );
        assert!(matching_suggestions("/model ", SUGGESTIONS).is_empty());
        let mut editor = Editor::default();
        editor.insert("/se");
        assert!(accept_suggestion(&mut editor, SUGGESTIONS));
        assert_eq!(editor.text(), "/search ");
    }

    #[test]
    fn decodes_terminal_navigation_shortcuts_and_bracketed_paste() {
        assert_eq!(decode_escape(b"\x1b[1;3D"), Action::WordLeft);
        assert_eq!(decode_escape(b"\x1b[1;9C"), Action::End);
        assert_eq!(decode_escape(b"\x1b[200~"), Action::PasteStart);
        assert_eq!(
            normalize_paste("한글\n질문".as_bytes()).unwrap(),
            "한글 질문"
        );
    }
}
