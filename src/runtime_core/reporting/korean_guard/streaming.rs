use super::{classification::classify_outside_text, validate, FAILURE};

const MAX_STREAM_GUARD_BUFFER_BYTES: usize = 1024 * 1024;

#[derive(Debug, Default)]
pub struct StreamingGuard {
    pending: String,
    held: String,
    fenced: bool,
    saw_hangul: bool,
}

impl StreamingGuard {
    pub fn push(&mut self, delta: &str) -> Result<String, &'static str> {
        self.pending.push_str(delta);
        self.check_buffer_limit()?;
        let mut output = String::new();
        while let Some(end) = self.next_unit_end() {
            let unit: String = self.pending.drain(..end).collect();
            output.push_str(&self.accept_unit(&unit)?);
        }
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<String, &'static str> {
        let mut output = String::new();
        if !self.pending.is_empty() {
            let unit = std::mem::take(&mut self.pending);
            output.push_str(&self.accept_unit(&unit)?);
        }
        if !self.saw_hangul {
            if validate(&self.held) {
                return Ok(std::mem::take(&mut self.held));
            }
            self.held.clear();
            return Err(FAILURE);
        }
        output.push_str(&std::mem::take(&mut self.held));
        Ok(output)
    }

    fn next_unit_end(&self) -> Option<usize> {
        if self.fenced || self.pending.trim_start().starts_with("```") {
            return self.pending.find('\n').map(|index| index + 1);
        }
        self.pending.char_indices().find_map(|(index, ch)| {
            matches!(ch, '\n' | '.' | '!' | '?' | '。' | '！' | '？')
                .then_some(index + ch.len_utf8())
        })
    }

    fn accept_unit(&mut self, unit: &str) -> Result<String, &'static str> {
        let fence_line = unit.trim().starts_with("```");
        if self.fenced || fence_line {
            if fence_line {
                self.fenced = !self.fenced;
            }
            return Ok(self.emit_or_hold(unit));
        }

        let line = classify_outside_text(unit);
        if line.forbidden {
            self.pending.clear();
            self.held.clear();
            return Err(FAILURE);
        }
        if line.has_hangul {
            self.saw_hangul = true;
        }
        Ok(self.emit_or_hold(unit))
    }

    fn emit_or_hold(&mut self, unit: &str) -> String {
        if self.saw_hangul {
            let mut output = std::mem::take(&mut self.held);
            output.push_str(unit);
            output
        } else {
            self.held.push_str(unit);
            String::new()
        }
    }

    fn check_buffer_limit(&self) -> Result<(), &'static str> {
        if self.pending.len().saturating_add(self.held.len()) > MAX_STREAM_GUARD_BUFFER_BYTES {
            Err(FAILURE)
        } else {
            Ok(())
        }
    }
}
