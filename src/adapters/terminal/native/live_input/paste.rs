use super::{TerminalFault, MAX_INPUT_BYTES};

const PASTE_END: &[u8] = b"\x1b[201~";

#[derive(Debug, Default)]
pub(super) struct PasteCapture {
    content: Vec<u8>,
    possible_end: Vec<u8>,
    overflowed: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct CompletedPaste {
    content: Vec<u8>,
    overflowed: bool,
}

impl PasteCapture {
    pub(super) fn push(&mut self, byte: u8) -> Option<CompletedPaste> {
        self.possible_end.push(byte);
        while !PASTE_END.starts_with(&self.possible_end) {
            let content_byte = self.possible_end.remove(0);
            if self.content.len() < MAX_INPUT_BYTES {
                self.content.push(content_byte);
            } else {
                self.overflowed = true;
            }
        }
        if self.possible_end != PASTE_END {
            return None;
        }
        Some(CompletedPaste {
            content: std::mem::take(&mut self.content),
            overflowed: self.overflowed,
        })
    }
}

pub(super) fn normalize(paste: &CompletedPaste) -> Result<String, TerminalFault> {
    match normalize_bytes(&paste.content) {
        Ok(value) => Ok(value),
        Err(TerminalFault::LineRead) if paste.overflowed => {
            let error = std::str::from_utf8(&paste.content).unwrap_err();
            if error.error_len().is_some() {
                return Err(TerminalFault::LineRead);
            }
            normalize_bytes(&paste.content[..error.valid_up_to()])
        }
        Err(error) => Err(error),
    }
}

fn normalize_bytes(bytes: &[u8]) -> Result<String, TerminalFault> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_multiline_paste() {
        let paste = CompletedPaste {
            content: "한글\n질문".as_bytes().to_vec(),
            overflowed: false,
        };

        assert_eq!(normalize(&paste).unwrap(), "한글 질문");
    }

    #[test]
    fn oversized_paste_still_consumes_the_end_marker() {
        let mut capture = PasteCapture::default();
        for byte in std::iter::repeat(b'a').take(MAX_INPUT_BYTES + 128) {
            assert!(capture.push(byte).is_none());
        }
        let mut completed = None;
        for byte in PASTE_END {
            completed = capture.push(*byte).or(completed);
        }

        let completed = completed.expect("paste end marker must remain observable after overflow");
        assert_eq!(completed.content.len(), MAX_INPUT_BYTES);
        assert!(completed.overflowed);
        assert_eq!(normalize(&completed).unwrap().len(), MAX_INPUT_BYTES);
    }

    #[test]
    fn truncated_multibyte_paste_keeps_only_complete_utf8() {
        let mut capture = PasteCapture::default();
        for byte in "가".repeat(MAX_INPUT_BYTES).bytes() {
            assert!(capture.push(byte).is_none());
        }
        let mut completed = None;
        for byte in PASTE_END {
            completed = capture.push(*byte).or(completed);
        }

        let completed = completed.expect("paste must finish");
        let normalized = normalize(&completed).unwrap();
        assert!(completed.overflowed);
        assert!(normalized.is_char_boundary(normalized.len()));
    }
}
