use std::io::{self, Read};

use super::{TerminalFault, TerminalInputEvent, TerminalSuggestion};

mod editor;
mod keymap;
mod paste;
mod picker;
mod render;

use editor::Editor;
use keymap::{decode_escape, escape_sequence_complete, Action};
use paste::PasteCapture;
pub(super) use picker::choose;
use render::BracketedPasteGuard;

const MAX_INPUT_BYTES: usize = 8 * 1024;
const MAX_PALETTE_ROWS: usize = 6;

pub(super) struct State {
    editor: Editor,
}

pub(super) struct ReadOutcome {
    pub(super) event: TerminalInputEvent,
    pub(super) state: Option<State>,
}

impl ReadOutcome {
    #[cfg(windows)]
    pub(super) fn from_line(line: Option<String>) -> Self {
        Self {
            event: line.map_or(TerminalInputEvent::End, TerminalInputEvent::Submit),
            state: None,
        }
    }
}

pub(super) fn read(
    suggestions: &[TerminalSuggestion],
    terminal_width: usize,
    base_frame: &str,
    state: Option<State>,
) -> Result<ReadOutcome, TerminalFault> {
    let _paste_guard = BracketedPasteGuard::start()?;
    let mut editor = state.map(|state| state.editor).unwrap_or_default();
    let mut escape = Vec::new();
    let mut utf8 = Vec::new();
    let mut paste = None::<PasteCapture>;
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
            if let Some(completed) = pasted.push(byte) {
                let normalized = paste::normalize(&completed)?;
                if normalized.is_empty() {
                    return Ok(clipboard_image_outcome(editor));
                }
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
                    paste = Some(PasteCapture::default());
                } else if matches!(action, Action::ScrollUp | Action::ScrollDown) {
                    return Ok(scroll_outcome(editor, action));
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
                return Ok(ReadOutcome {
                    event: TerminalInputEvent::Submit(editor.into_text()),
                    state: None,
                });
            }
            0x03 => {
                return Ok(ReadOutcome {
                    event: TerminalInputEvent::End,
                    state: None,
                })
            }
            0x04 if editor.text().is_empty() => {
                return Ok(ReadOutcome {
                    event: TerminalInputEvent::End,
                    state: None,
                })
            }
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
            0x16 => return Ok(clipboard_image_outcome(editor)),
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

fn clipboard_image_outcome(editor: Editor) -> ReadOutcome {
    ReadOutcome {
        event: TerminalInputEvent::PasteClipboardImage,
        state: Some(State { editor }),
    }
}

fn scroll_outcome(editor: Editor, action: Action) -> ReadOutcome {
    let event = match action {
        Action::ScrollUp => TerminalInputEvent::ScrollUp,
        Action::ScrollDown => TerminalInputEvent::ScrollDown,
        _ => unreachable!("scroll outcome requires a scroll action"),
    };
    ReadOutcome {
        event,
        state: Some(State { editor }),
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
        Action::PasteStart | Action::ScrollUp | Action::ScrollDown | Action::Ignore => {}
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
