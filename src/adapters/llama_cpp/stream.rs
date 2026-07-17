use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use crate::foundation::error::AppError;
use crate::runtime_core::inference::stream::{StreamCompletion, StreamOutcome, StreamTermination};

const READ_POLL_INTERVAL: Duration = Duration::from_millis(100);

mod protocol;

use protocol::{ChatSseDecoder, HttpResponseDecoder};
#[cfg(test)]
use protocol::{
    ReasoningTraceFilter, MAX_COMPLETION_BYTES, MAX_HTTP_BODY_BUFFER_BYTES, MAX_HTTP_CHUNK_BYTES,
};

pub(crate) fn post_chat_stream(
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

#[cfg(test)]
#[path = "stream/tests.rs"]
mod tests;
