use std::time::Duration;

use crate::foundation::error::AppError;
use crate::foundation::serialization as strict_json;
use crate::runtime_core::inference::stream::StreamCompletion;
use strict_json::{Object, Value};

const MAX_HTTP_HEADERS_BYTES: usize = 64 * 1024;
const MAX_SSE_EVENT_BYTES: usize = 1024 * 1024;
pub(super) const MAX_HTTP_CHUNK_BYTES: usize = MAX_SSE_EVENT_BYTES;
pub(super) const MAX_HTTP_BODY_BUFFER_BYTES: usize = MAX_SSE_EVENT_BYTES + 64 * 1024;
pub(super) const MAX_COMPLETION_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Default)]
pub(super) struct HttpResponseDecoder {
    buffer: Vec<u8>,
    status_code: Option<u16>,
    headers_complete: bool,
    chunked: bool,
    chunk_remaining: Option<usize>,
    pub(super) body_complete: bool,
}

impl HttpResponseDecoder {
    pub(super) fn push(&mut self, bytes: &[u8]) -> Result<Vec<Vec<u8>>, AppError> {
        self.buffer.extend_from_slice(bytes);
        if !self.headers_complete {
            if self.buffer.len() > MAX_HTTP_HEADERS_BYTES {
                return Err(AppError::blocked(
                    "backend response header가 허용 크기를 초과했습니다.",
                ));
            }
            let Some(header_end) = find_subsequence(&self.buffer, b"\r\n\r\n") else {
                return Ok(Vec::new());
            };
            let header_bytes = self.buffer[..header_end].to_vec();
            let headers = String::from_utf8(header_bytes).map_err(|_| {
                AppError::blocked("backend response header가 유효한 UTF-8이 아닙니다.")
            })?;
            let status_code = headers
                .lines()
                .next()
                .and_then(|line| line.split_ascii_whitespace().nth(1))
                .and_then(|value| value.parse::<u16>().ok())
                .filter(|value| (100..=599).contains(value))
                .ok_or_else(|| {
                    AppError::blocked("backend response status line 형식이 유효하지 않습니다.")
                })?;
            self.status_code = Some(status_code);
            self.chunked = headers.lines().skip(1).any(|line| {
                line.split_once(':')
                    .map(|(name, value)| {
                        name.trim().eq_ignore_ascii_case("transfer-encoding")
                            && value
                                .split(',')
                                .any(|encoding| encoding.trim().eq_ignore_ascii_case("chunked"))
                    })
                    .unwrap_or(false)
            });
            self.buffer.drain(..header_end + 4);
            self.headers_complete = true;
        }

        if self.failed_status_code().is_some() || self.body_complete {
            return Ok(Vec::new());
        }
        if self.buffer.len() > MAX_HTTP_BODY_BUFFER_BYTES {
            return Err(AppError::blocked(
                "backend response body buffer가 허용 크기를 초과했습니다.",
            ));
        }
        if !self.chunked {
            if self.buffer.is_empty() {
                return Ok(Vec::new());
            }
            return Ok(vec![std::mem::take(&mut self.buffer)]);
        }

        let mut output = Vec::new();
        loop {
            if self.chunk_remaining.is_none() {
                let Some(line_end) = find_subsequence(&self.buffer, b"\r\n") else {
                    break;
                };
                let size_line =
                    String::from_utf8(self.buffer[..line_end].to_vec()).map_err(|_| {
                        AppError::blocked("backend chunk size line이 유효한 UTF-8이 아닙니다.")
                    })?;
                let size_hex = size_line.split(';').next().unwrap_or("").trim();
                let size = usize::from_str_radix(size_hex, 16).map_err(|_| {
                    AppError::blocked(format!(
                        "backend chunk size를 해석하지 못했습니다: {size_line}"
                    ))
                })?;
                if size > MAX_HTTP_CHUNK_BYTES {
                    return Err(AppError::blocked(format!(
                        "backend chunk가 허용 크기를 초과했습니다: {size} bytes"
                    )));
                }
                self.buffer.drain(..line_end + 2);
                if size == 0 {
                    self.body_complete = true;
                    break;
                }
                self.chunk_remaining = Some(size);
            }

            let size = self.chunk_remaining.unwrap_or(0);
            let framed_size = size.checked_add(2).ok_or_else(|| {
                AppError::blocked("backend chunk framing 크기가 overflow되었습니다.")
            })?;
            if self.buffer.len() < framed_size {
                break;
            }
            if &self.buffer[size..size + 2] != b"\r\n" {
                return Err(AppError::blocked(
                    "backend chunk framing의 CRLF가 유효하지 않습니다.",
                ));
            }
            output.push(self.buffer[..size].to_vec());
            self.buffer.drain(..size + 2);
            self.chunk_remaining = None;
        }
        Ok(output)
    }

    pub(super) fn failed_status_code(&self) -> Option<u16> {
        self.status_code.filter(|code| *code != 200)
    }
}

#[derive(Debug, Default)]
pub(super) struct ChatSseDecoder {
    pub(super) buffer: Vec<u8>,
    pub(super) content: String,
    pub(super) finish_reason: Option<String>,
    pub(super) prompt_tokens: Option<u32>,
    pub(super) completion_tokens: Option<u32>,
    pub(super) total_tokens: Option<u32>,
    pub(super) first_token_latency_ms: Option<u128>,
    pub(super) reasoning_filter: ReasoningTraceFilter,
    pub(super) done: bool,
}

impl ChatSseDecoder {
    pub(super) fn push(
        &mut self,
        bytes: &[u8],
        elapsed: Duration,
        on_delta: &mut impl FnMut(&str) -> Result<(), AppError>,
    ) -> Result<(), AppError> {
        self.buffer.extend_from_slice(bytes);
        if self.buffer.len() > MAX_SSE_EVENT_BYTES {
            return Err(AppError::blocked(
                "backend SSE event가 허용 크기를 초과했습니다.",
            ));
        }
        while let Some(event_end) = find_sse_event_end(&self.buffer) {
            let event = self.buffer[..event_end].to_vec();
            let separator_len = if self.buffer.get(event_end..event_end + 4) == Some(b"\r\n\r\n") {
                4
            } else {
                2
            };
            self.buffer.drain(..event_end + separator_len);
            self.consume_event(&event, elapsed, on_delta)?;
            if self.done {
                break;
            }
        }
        Ok(())
    }

    fn consume_event(
        &mut self,
        event: &[u8],
        elapsed: Duration,
        on_delta: &mut impl FnMut(&str) -> Result<(), AppError>,
    ) -> Result<(), AppError> {
        let event = String::from_utf8(event.to_vec())
            .map_err(|_| AppError::blocked("backend SSE event가 유효한 UTF-8이 아닙니다."))?;
        let data = event
            .lines()
            .filter_map(|line| {
                let line = line.trim_end_matches('\r');
                line.strip_prefix("data:")
                    .map(|value| value.strip_prefix(' ').unwrap_or(value))
            })
            .collect::<Vec<_>>()
            .join("\n");
        if data.is_empty() {
            return Ok(());
        }
        if data == "[DONE]" {
            self.done = true;
            return Ok(());
        }
        let value = strict_json::parse_value(&data, "backend SSE event")
            .map_err(|_| malformed_sse_event())?;
        let Value::Object(object) = value else {
            return Err(malformed_sse_event());
        };
        if object.contains_key("error") {
            return Err(AppError::blocked(
                "backend streaming response 오류\n- category: upstream-error-event",
            ));
        }
        let choice = first_choice(&object)?;

        if let Some(choice) = choice {
            match choice.get("finish_reason") {
                Some(Value::String(reason)) => self.finish_reason = Some(reason.clone()),
                Some(Value::Null) | None => {}
                Some(_) => return Err(malformed_sse_event()),
            }
        }
        if let Some(Value::Object(usage)) = object.get("usage") {
            self.prompt_tokens = json_u32(usage, "prompt_tokens")?.or(self.prompt_tokens);
            self.completion_tokens =
                json_u32(usage, "completion_tokens")?.or(self.completion_tokens);
            self.total_tokens = json_u32(usage, "total_tokens")?.or(self.total_tokens);
        } else if object.contains_key("usage") && !matches!(object.get("usage"), Some(Value::Null))
        {
            return Err(malformed_sse_event());
        }

        if let Some(delta) = choice.map(choice_content).transpose()?.flatten() {
            let safe = self.reasoning_filter.push(&delta);
            if !safe.is_empty() {
                self.ensure_completion_capacity(safe.len())?;
                if self.first_token_latency_ms.is_none() {
                    self.first_token_latency_ms = Some(elapsed.as_millis());
                }
                self.content.push_str(&safe);
                on_delta(&safe)?;
            }
        }
        Ok(())
    }

    pub(super) fn finish(
        &mut self,
        on_delta: &mut impl FnMut(&str) -> Result<(), AppError>,
    ) -> Result<(), AppError> {
        let safe = self.reasoning_filter.finish();
        if !safe.is_empty() {
            self.ensure_completion_capacity(safe.len())?;
            self.content.push_str(&safe);
            on_delta(&safe)?;
        }
        Ok(())
    }

    fn ensure_completion_capacity(&self, additional: usize) -> Result<(), AppError> {
        if self.content.len().saturating_add(additional) > MAX_COMPLETION_BYTES {
            return Err(AppError::blocked(
                "backend filtered completion이 허용 크기를 초과했습니다.",
            ));
        }
        Ok(())
    }

    pub(super) fn completion(&self) -> StreamCompletion {
        StreamCompletion {
            content: self.content.clone(),
            finish_reason: self
                .finish_reason
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
            prompt_tokens: self.prompt_tokens,
            completion_tokens: self.completion_tokens,
            total_tokens: self.total_tokens,
            first_token_latency_ms: self.first_token_latency_ms,
            had_reasoning_trace: self.reasoning_filter.had_reasoning_trace,
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct ReasoningTraceFilter {
    pending: String,
    inside_reasoning: bool,
    pub(super) had_reasoning_trace: bool,
}

impl ReasoningTraceFilter {
    pub(super) fn push(&mut self, delta: &str) -> String {
        self.pending.push_str(delta);
        let mut output = String::new();
        loop {
            if self.inside_reasoning {
                if let Some(end) = self.pending.find("</think>") {
                    self.pending.drain(..end + "</think>".len());
                    self.inside_reasoning = false;
                    continue;
                }
                let keep = longest_suffix_prefix(&self.pending, "</think>");
                let drop_len = self.pending.len().saturating_sub(keep);
                self.pending.drain(..drop_len);
                break;
            }

            if let Some(start) = self.pending.find("<think>") {
                output.push_str(&self.pending[..start]);
                self.pending.drain(..start + "<think>".len());
                self.inside_reasoning = true;
                self.had_reasoning_trace = true;
                continue;
            }
            let keep = longest_suffix_prefix(&self.pending, "<think>");
            let emit_len = self.pending.len().saturating_sub(keep);
            output.push_str(&self.pending[..emit_len]);
            self.pending.drain(..emit_len);
            break;
        }
        output
    }

    pub(super) fn finish(&mut self) -> String {
        if self.inside_reasoning {
            self.pending.clear();
            String::new()
        } else {
            std::mem::take(&mut self.pending)
        }
    }
}

fn longest_suffix_prefix(value: &str, marker: &str) -> usize {
    let max = value.len().min(marker.len().saturating_sub(1));
    (1..=max)
        .rev()
        .find(|length| {
            value.is_char_boundary(value.len() - length)
                && marker.is_char_boundary(*length)
                && value[value.len() - length..] == marker[..*length]
        })
        .unwrap_or(0)
}

fn find_sse_event_end(bytes: &[u8]) -> Option<usize> {
    find_subsequence(bytes, b"\r\n\r\n").or_else(|| find_subsequence(bytes, b"\n\n"))
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn malformed_sse_event() -> AppError {
    AppError::blocked("backend streaming response 오류\n- category: upstream-malformed-event")
}

fn first_choice(object: &Object) -> Result<Option<&Object>, AppError> {
    let Some(choices) = object.get("choices") else {
        return Ok(None);
    };
    let Value::Array(choices) = choices else {
        return Err(malformed_sse_event());
    };
    let Some(choice) = choices.first() else {
        return Ok(None);
    };
    let Value::Object(choice) = choice else {
        return Err(malformed_sse_event());
    };
    Ok(Some(choice))
}

fn choice_content(choice: &Object) -> Result<Option<String>, AppError> {
    let Some(delta) = choice.get("delta") else {
        return Ok(None);
    };
    let Value::Object(delta) = delta else {
        return Err(malformed_sse_event());
    };
    match delta.get("content") {
        Some(Value::String(content)) => Ok(Some(content.clone())),
        Some(Value::Null) | None => Ok(None),
        Some(_) => Err(malformed_sse_event()),
    }
}

fn json_u32(object: &Object, key: &str) -> Result<Option<u32>, AppError> {
    match object.get(key) {
        Some(Value::Number(value)) => u32::try_from(*value)
            .map(Some)
            .map_err(|_| malformed_sse_event()),
        Some(_) => Err(malformed_sse_event()),
        None => Ok(None),
    }
}
