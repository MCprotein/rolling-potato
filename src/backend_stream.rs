use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use crate::app::AppError;
use crate::strict_json::{Object, Value};

const READ_POLL_INTERVAL: Duration = Duration::from_millis(100);
const MAX_HTTP_HEADERS_BYTES: usize = 64 * 1024;
const MAX_SSE_EVENT_BYTES: usize = 1024 * 1024;
const MAX_HTTP_CHUNK_BYTES: usize = MAX_SSE_EVENT_BYTES;
const MAX_HTTP_BODY_BUFFER_BYTES: usize = MAX_SSE_EVENT_BYTES + 64 * 1024;
const MAX_COMPLETION_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamTermination {
    Completed,
    Cancelled,
    TimedOut,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct StreamCompletion {
    pub content: String,
    pub finish_reason: String,
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
    pub total_tokens: Option<u32>,
    pub first_token_latency_ms: Option<u128>,
    pub had_reasoning_trace: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StreamOutcome {
    pub termination: StreamTermination,
    pub completion: StreamCompletion,
}

pub fn post_chat_stream(
    host: &str,
    port: u16,
    path: &str,
    body: &str,
    timeout: Duration,
    mut cancel_requested: impl FnMut() -> Result<bool, AppError>,
    mut on_delta: impl FnMut(&str) -> Result<(), AppError>,
) -> Result<StreamOutcome, AppError> {
    let started_at = Instant::now();
    if cancel_requested()? {
        return Ok(empty_outcome(StreamTermination::Cancelled));
    }
    let address = format!("{host}:{port}");
    let mut addresses = address.to_socket_addrs().map_err(|err| {
        AppError::runtime(format!("backend address resolve 실패: {address} ({err})"))
    })?;
    let socket_addr = addresses
        .next()
        .ok_or_else(|| AppError::runtime(format!("backend address 없음: {address}")))?;
    let Some(connect_timeout) = remaining_timeout(started_at, timeout) else {
        return Ok(empty_outcome(StreamTermination::TimedOut));
    };
    let mut stream = match TcpStream::connect_timeout(&socket_addr, connect_timeout) {
        Ok(stream) => stream,
        Err(err) if matches!(err.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
            return Ok(empty_outcome(StreamTermination::TimedOut));
        }
        Err(err) => {
            return Err(AppError::runtime(format!(
                "backend 연결 실패: {socket_addr} ({err})"
            )));
        }
    };
    if cancel_requested()? {
        let _ = stream.shutdown(Shutdown::Both);
        return Ok(empty_outcome(StreamTermination::Cancelled));
    }
    let Some(write_timeout) = remaining_timeout(started_at, timeout) else {
        let _ = stream.shutdown(Shutdown::Both);
        return Ok(empty_outcome(StreamTermination::TimedOut));
    };
    let _ = stream.set_read_timeout(Some(READ_POLL_INTERVAL.min(write_timeout)));

    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {host}:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nAccept: text/event-stream\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    if let Some(termination) = write_request_with_polling(
        &mut stream,
        request.as_bytes(),
        started_at,
        timeout,
        &mut cancel_requested,
    )? {
        let _ = stream.shutdown(Shutdown::Both);
        return Ok(empty_outcome(termination));
    }

    let mut http = HttpResponseDecoder::default();
    let mut sse = ChatSseDecoder::default();
    let mut read_buffer = [0_u8; 16 * 1024];

    loop {
        if cancel_requested()? {
            let _ = stream.shutdown(Shutdown::Both);
            sse.finish(&mut on_delta)?;
            return Ok(StreamOutcome {
                termination: StreamTermination::Cancelled,
                completion: sse.completion(),
            });
        }
        if started_at.elapsed() >= timeout {
            let _ = stream.shutdown(Shutdown::Both);
            sse.finish(&mut on_delta)?;
            return Ok(StreamOutcome {
                termination: StreamTermination::TimedOut,
                completion: sse.completion(),
            });
        }

        match stream.read(&mut read_buffer) {
            Ok(0) => {
                sse.finish(&mut on_delta)?;
                if sse.done {
                    return Ok(StreamOutcome {
                        termination: StreamTermination::Completed,
                        completion: sse.completion(),
                    });
                }
                return Err(AppError::runtime(
                    "backend streaming response가 [DONE] 전에 종료되었습니다.",
                ));
            }
            Ok(read_bytes) => {
                for body_chunk in http.push(&read_buffer[..read_bytes])? {
                    sse.push(&body_chunk, started_at.elapsed(), &mut on_delta)?;
                }
                if let Some(status_code) = http.failed_status_code() {
                    return Err(AppError::blocked(format!(
                        "backend request 실패\n- endpoint: {path}\n- status code: {status_code}"
                    )));
                }
                if sse.done {
                    sse.finish(&mut on_delta)?;
                    return Ok(StreamOutcome {
                        termination: StreamTermination::Completed,
                        completion: sse.completion(),
                    });
                }
            }
            Err(err) if matches!(err.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(err) => {
                return Err(AppError::runtime(format!(
                    "backend streaming response read 실패: {err}"
                )));
            }
        }
    }
}

fn write_request_with_polling(
    stream: &mut TcpStream,
    request: &[u8],
    started_at: Instant,
    timeout: Duration,
    cancel_requested: &mut impl FnMut() -> Result<bool, AppError>,
) -> Result<Option<StreamTermination>, AppError> {
    let mut written = 0;
    while written < request.len() {
        if cancel_requested()? {
            return Ok(Some(StreamTermination::Cancelled));
        }
        let Some(remaining) = remaining_timeout(started_at, timeout) else {
            return Ok(Some(StreamTermination::TimedOut));
        };
        stream
            .set_write_timeout(Some(READ_POLL_INTERVAL.min(remaining)))
            .map_err(|err| {
                AppError::runtime(format!("backend request write timeout 설정 실패: {err}"))
            })?;
        match stream.write(&request[written..]) {
            Ok(0) => {
                return Err(AppError::runtime(
                    "backend request write가 완료 전에 종료되었습니다.",
                ));
            }
            Ok(bytes) => written += bytes,
            Err(err) if matches!(err.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(err) => {
                return Err(AppError::runtime(format!(
                    "backend request write 실패: {err}"
                )));
            }
        }
    }
    Ok(None)
}

fn remaining_timeout(started_at: Instant, timeout: Duration) -> Option<Duration> {
    timeout
        .checked_sub(started_at.elapsed())
        .filter(|remaining| !remaining.is_zero())
}

fn empty_outcome(termination: StreamTermination) -> StreamOutcome {
    StreamOutcome {
        termination,
        completion: StreamCompletion::default(),
    }
}

#[derive(Debug, Default)]
struct HttpResponseDecoder {
    buffer: Vec<u8>,
    status_code: Option<u16>,
    headers_complete: bool,
    chunked: bool,
    chunk_remaining: Option<usize>,
    body_complete: bool,
}

impl HttpResponseDecoder {
    fn push(&mut self, bytes: &[u8]) -> Result<Vec<Vec<u8>>, AppError> {
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

    fn failed_status_code(&self) -> Option<u16> {
        self.status_code.filter(|code| *code != 200)
    }
}

#[derive(Debug, Default)]
struct ChatSseDecoder {
    buffer: Vec<u8>,
    content: String,
    finish_reason: Option<String>,
    prompt_tokens: Option<u32>,
    completion_tokens: Option<u32>,
    total_tokens: Option<u32>,
    first_token_latency_ms: Option<u128>,
    reasoning_filter: ReasoningTraceFilter,
    done: bool,
}

impl ChatSseDecoder {
    fn push(
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
        let value = crate::strict_json::parse_value(&data, "backend SSE event")
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

    fn finish(
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

    fn completion(&self) -> StreamCompletion {
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
struct ReasoningTraceFilter {
    pending: String,
    inside_reasoning: bool,
    had_reasoning_trace: bool,
}

impl ReasoningTraceFilter {
    fn push(&mut self, delta: &str) -> String {
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

    fn finish(&mut self) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{mpsc, Arc};
    use std::thread;

    #[test]
    fn decodes_split_chunked_http_body() {
        let response = concat!(
            "HTTP/1.1 200 OK\r\n",
            "Content-Type: text/event-stream\r\n",
            "Transfer-Encoding: chunked\r\n\r\n",
            "5\r\ndata:\r\n",
            "8\r\n hello\n\n\r\n",
            "0\r\n\r\n"
        );
        let mut decoder = HttpResponseDecoder::default();
        let mut body = Vec::new();
        for part in response.as_bytes().chunks(3) {
            for chunk in decoder.push(part).unwrap() {
                body.extend_from_slice(&chunk);
            }
        }

        assert_eq!(body, b"data: hello\n\n");
        assert!(decoder.body_complete);
    }

    #[test]
    fn accepts_chunk_extension_and_trailer() {
        let response = concat!(
            "HTTP/1.1 200 OK\r\n",
            "Content-Type: text/event-stream\r\n",
            "Transfer-Encoding: chunked\r\n\r\n",
            "5;source=test\r\ndata:\r\n",
            "8\r\n hello\n\n\r\n",
            "0\r\nX-Trace: complete\r\n\r\n"
        );
        let mut decoder = HttpResponseDecoder::default();
        let mut body = Vec::new();
        for part in response.as_bytes().chunks(2) {
            for chunk in decoder.push(part).unwrap() {
                body.extend_from_slice(&chunk);
            }
        }

        assert_eq!(body, b"data: hello\n\n");
        assert!(decoder.body_complete);
    }

    #[test]
    fn decodes_sse_content_usage_and_done_across_boundaries() {
        let events = concat!(
            "data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"delta\":{\"content\":\"감\"},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"자\"},\"finish_reason\":\"stop\"}]}\n\n",
            "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":11,\"completion_tokens\":2,\"total_tokens\":13}}\n\n",
            "data: [DONE]\n\n"
        );
        let mut decoder = ChatSseDecoder::default();
        let mut streamed = String::new();
        for part in events.as_bytes().chunks(7) {
            decoder
                .push(part, Duration::from_millis(42), &mut |delta| {
                    streamed.push_str(delta);
                    Ok(())
                })
                .unwrap();
        }
        decoder.finish(&mut |_| Ok(())).unwrap();
        let completion = decoder.completion();

        assert!(decoder.done);
        assert_eq!(streamed, "감자");
        assert_eq!(completion.content, "감자");
        assert_eq!(completion.finish_reason, "stop");
        assert_eq!(completion.prompt_tokens, Some(11));
        assert_eq!(completion.completion_tokens, Some(2));
        assert_eq!(completion.total_tokens, Some(13));
        assert_eq!(completion.first_token_latency_ms, Some(42));
    }

    #[test]
    fn accepts_multiline_data_and_discards_reasoning_content() {
        let events = concat!(
            "data: {\"reasoning_content\":\"secret\",\"choices\":[\n",
            "data: {\"delta\":{\"content\":\"answer\"},\"finish_reason\":\"stop\"}]}\n\n",
            "data: [DONE]\n\n"
        );
        let mut decoder = ChatSseDecoder::default();
        let mut streamed = String::new();
        decoder
            .push(events.as_bytes(), Duration::from_millis(7), &mut |delta| {
                streamed.push_str(delta);
                Ok(())
            })
            .unwrap();

        assert!(decoder.done);
        assert_eq!(streamed, "answer");
        assert_eq!(decoder.completion().content, "answer");
        assert!(!streamed.contains("secret"));
    }

    #[test]
    fn rejects_stream_error_event() {
        let mut decoder = ChatSseDecoder::default();
        let error = decoder
            .push(
                b"data: {\"error\":{\"message\":\"model unavailable\"}}\n\n",
                Duration::from_millis(1),
                &mut |_| Ok(()),
            )
            .unwrap_err();

        assert!(error.message.contains("streaming response"));
        assert!(error.message.contains("upstream-error-event"));
        assert!(!error.message.contains("model unavailable"));
    }

    #[test]
    fn rejects_whitespace_variant_error_without_emitting_nested_content() {
        let mut decoder = ChatSseDecoder::default();
        let mut streamed = String::new();
        let error = decoder
            .push(
                b"data: {\"error\" : {\"content\":\"sensitive detail\"}}\n\n",
                Duration::from_millis(1),
                &mut |delta| {
                    streamed.push_str(delta);
                    Ok(())
                },
            )
            .unwrap_err();

        assert!(error.message.contains("upstream-error-event"));
        assert!(!error.message.contains("sensitive detail"));
        assert!(streamed.is_empty());
        assert!(decoder.content.is_empty());
    }

    #[test]
    fn rejects_oversized_and_overflowing_chunk_declarations() {
        for size in [
            format!("{:X}", MAX_HTTP_CHUNK_BYTES + 1),
            format!("{:X}", usize::MAX),
        ] {
            let response =
                format!("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{size}\r\n");
            let mut decoder = HttpResponseDecoder::default();
            let error = decoder.push(response.as_bytes()).unwrap_err();

            assert!(error.message.contains("허용 크기"));
        }
    }

    #[test]
    fn rejects_incomplete_body_buffer_over_limit() {
        let mut decoder = HttpResponseDecoder::default();
        decoder
            .push(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n100000\r\n")
            .unwrap();
        let error = decoder
            .push(&vec![b'x'; MAX_HTTP_BODY_BUFFER_BYTES + 1])
            .unwrap_err();

        assert!(error.message.contains("body buffer"));
    }

    #[test]
    fn rejects_many_valid_events_over_total_completion_limit() {
        let mut decoder = ChatSseDecoder::default();
        let chunk = "가".repeat(300_000);
        let event =
            format!("data: {{\"choices\":[{{\"delta\":{{\"content\":\"{chunk}\"}}}}]}}\n\n");
        let mut error = None;
        for _ in 0..3 {
            match decoder.push(event.as_bytes(), Duration::from_millis(1), &mut |_| Ok(())) {
                Ok(()) => {}
                Err(err) => {
                    error = Some(err);
                    break;
                }
            }
        }

        let error = error.expect("누적 completion 제한을 초과해야 합니다.");
        assert!(error.message.contains("filtered completion"));
        assert!(decoder.content.len() <= MAX_COMPLETION_BYTES);
    }

    #[test]
    fn reasoning_filter_finish_cannot_exceed_total_completion_limit() {
        let mut decoder = ChatSseDecoder {
            content: "x".repeat(MAX_COMPLETION_BYTES),
            ..ChatSseDecoder::default()
        };
        assert!(decoder.reasoning_filter.push("<thin").is_empty());
        let mut streamed = String::new();

        let error = decoder
            .finish(&mut |delta| {
                streamed.push_str(delta);
                Ok(())
            })
            .unwrap_err();

        assert!(error.message.contains("filtered completion"));
        assert!(streamed.is_empty());
        assert_eq!(decoder.content.len(), MAX_COMPLETION_BYTES);
    }

    #[test]
    fn total_timeout_includes_work_before_connect() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let started = Instant::now();
        let mut checks = 0;

        let outcome = post_chat_stream(
            "127.0.0.1",
            port,
            "/v1/chat/completions",
            "{}",
            Duration::from_millis(10),
            || {
                checks += 1;
                if checks == 1 {
                    thread::sleep(Duration::from_millis(25));
                }
                Ok(false)
            },
            |_| Ok(()),
        )
        .unwrap();

        assert_eq!(outcome.termination, StreamTermination::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn reasoning_filter_never_emits_split_think_trace() {
        let mut filter = ReasoningTraceFilter::default();
        let mut output = String::new();
        for delta in ["<thi", "nk>비밀", " 추론</th", "ink>최종", " 답변"] {
            output.push_str(&filter.push(delta));
        }
        output.push_str(&filter.finish());

        assert_eq!(output, "최종 답변");
        assert!(filter.had_reasoning_trace);
        assert!(!output.contains("비밀"));
    }

    #[test]
    fn reasoning_filter_preserves_normal_marker_like_text() {
        let mut filter = ReasoningTraceFilter::default();
        let mut output = filter.push("값은 <thin");
        output.push_str(&filter.push("gs>와 다릅니다."));
        output.push_str(&filter.finish());

        assert_eq!(output, "값은 <things>와 다릅니다.");
        assert!(!filter.had_reasoning_trace);
    }

    #[test]
    fn first_token_latency_starts_at_first_visible_delta() {
        let mut decoder = ChatSseDecoder::default();
        decoder
            .push(
                b"data: {\"choices\":[{\"delta\":{\"content\":\"<think>secret\"}}]}\n\n",
                Duration::from_millis(10),
                &mut |_| Ok(()),
            )
            .unwrap();
        decoder
            .push(
                b"data: {\"choices\":[{\"delta\":{\"content\":\"</think>answer\"}}]}\n\n",
                Duration::from_millis(50),
                &mut |_| Ok(()),
            )
            .unwrap();

        assert_eq!(decoder.completion().first_token_latency_ms, Some(50));
        assert_eq!(decoder.completion().content, "answer");
    }

    #[test]
    fn streams_chunked_sse_over_tcp() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            read_http_request(&mut socket);
            let sse = concat!(
                "data: {\"choices\":[{\"delta\":{\"content\":\"감자\"},\"finish_reason\":\"stop\"}]}\n\n",
                "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":1,\"total_tokens\":4}}\n\n",
                "data: [DONE]\n\n"
            );
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n{:X}\r\n{}\r\n0\r\n\r\n",
                sse.len(),
                sse
            );
            for part in response.as_bytes().chunks(11) {
                socket.write_all(part).unwrap();
            }
        });
        let mut streamed = String::new();

        let outcome = post_chat_stream(
            "127.0.0.1",
            port,
            "/v1/chat/completions",
            "{}",
            Duration::from_secs(2),
            || Ok(false),
            |delta| {
                streamed.push_str(delta);
                Ok(())
            },
        )
        .unwrap();
        server.join().unwrap();

        assert_eq!(outcome.termination, StreamTermination::Completed);
        assert_eq!(streamed, "감자");
        assert_eq!(outcome.completion.total_tokens, Some(4));
    }

    #[test]
    fn cancellation_closes_client_socket_without_waiting_for_server() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (closed_tx, closed_rx) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancellation_flag = Arc::clone(&cancelled);
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            read_http_request(&mut socket);
            cancellation_flag.store(true, Ordering::Release);
            socket
                .set_read_timeout(Some(Duration::from_secs(1)))
                .unwrap();
            let mut byte = [0_u8; 1];
            closed_tx
                .send(socket_close_result(socket.read(&mut byte)))
                .unwrap();
        });

        let outcome = post_chat_stream(
            "127.0.0.1",
            port,
            "/v1/chat/completions",
            "{}",
            Duration::from_secs(2),
            || Ok(cancelled.load(Ordering::Acquire)),
            |_| Ok(()),
        )
        .unwrap();
        server.join().unwrap();

        assert_eq!(outcome.termination, StreamTermination::Cancelled);
        closed_rx.recv().unwrap().unwrap();
    }

    #[test]
    fn cancellation_interrupts_a_stalled_request_upload() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (accepted_tx, accepted_rx) = mpsc::channel();
        let (closed_tx, closed_rx) = mpsc::channel();
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            accepted_tx.send(()).unwrap();
            thread::sleep(Duration::from_millis(300));
            closed_tx
                .send(socket_eventually_closed(&mut socket))
                .unwrap();
        });
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancellation_flag = Arc::clone(&cancelled);
        let canceller = thread::spawn(move || {
            accepted_rx.recv().unwrap();
            thread::sleep(Duration::from_millis(50));
            cancellation_flag.store(true, Ordering::Release);
        });
        let body = "x".repeat(32 * 1024 * 1024);
        let started = Instant::now();

        let outcome = post_chat_stream(
            "127.0.0.1",
            port,
            "/v1/chat/completions",
            &body,
            Duration::from_secs(2),
            || Ok(cancelled.load(Ordering::Acquire)),
            |_| Ok(()),
        )
        .unwrap();
        let elapsed = started.elapsed();
        canceller.join().unwrap();
        server.join().unwrap();

        assert_eq!(outcome.termination, StreamTermination::Cancelled);
        assert!(elapsed < Duration::from_millis(500));
        closed_rx.recv().unwrap().unwrap();
    }

    #[test]
    fn total_timeout_closes_stalled_stream() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (closed_tx, closed_rx) = mpsc::channel();
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            read_http_request(&mut socket);
            socket
                .set_read_timeout(Some(Duration::from_secs(1)))
                .unwrap();
            let mut byte = [0_u8; 1];
            closed_tx
                .send(socket_close_result(socket.read(&mut byte)))
                .unwrap();
        });

        let outcome = post_chat_stream(
            "127.0.0.1",
            port,
            "/v1/chat/completions",
            "{}",
            Duration::from_millis(50),
            || Ok(false),
            |_| Ok(()),
        )
        .unwrap();
        server.join().unwrap();

        assert_eq!(outcome.termination, StreamTermination::TimedOut);
        closed_rx.recv().unwrap().unwrap();
    }

    fn socket_close_result(result: std::io::Result<usize>) -> Result<(), String> {
        match result {
            Ok(0) => Ok(()),
            Err(err)
                if matches!(
                    err.kind(),
                    ErrorKind::ConnectionReset
                        | ErrorKind::ConnectionAborted
                        | ErrorKind::BrokenPipe
                ) =>
            {
                Ok(())
            }
            Ok(bytes) => Err(format!("socket에 종료 후에도 {bytes} byte가 도착했습니다.")),
            Err(err) => Err(format!("socket 종료 대신 read 오류가 발생했습니다: {err}")),
        }
    }

    fn socket_eventually_closed(socket: &mut TcpStream) -> Result<(), String> {
        socket
            .set_read_timeout(Some(Duration::from_secs(1)))
            .map_err(|err| err.to_string())?;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            match socket.read(&mut buffer) {
                Ok(0) => return Ok(()),
                Ok(_) => {}
                Err(err)
                    if matches!(
                        err.kind(),
                        ErrorKind::ConnectionReset
                            | ErrorKind::ConnectionAborted
                            | ErrorKind::BrokenPipe
                    ) =>
                {
                    return Ok(());
                }
                Err(err) => {
                    return Err(format!(
                        "buffered upload를 비운 뒤 socket 종료를 확인하지 못했습니다: {err}"
                    ));
                }
            }
        }
    }

    fn read_http_request(socket: &mut TcpStream) {
        socket
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut request = Vec::new();
        let mut buffer = [0_u8; 1024];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            let read = socket.read(&mut buffer).unwrap();
            assert!(read > 0);
            request.extend_from_slice(&buffer[..read]);
        }
        assert!(String::from_utf8_lossy(&request).contains("Accept: text/event-stream"));
    }
}
