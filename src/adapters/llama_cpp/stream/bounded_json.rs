//! Bounded, cancellable JSON transport for backend capability preflights.

use std::io::{ErrorKind, Read};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use crate::foundation::error::AppError;
use crate::runtime_core::inference::stream::StreamTermination;

use super::protocol::HttpResponseDecoder;
use super::{remaining_timeout, write_request_with_polling, READ_POLL_INTERVAL};

const MAX_RESPONSE_BYTES: usize = 64 * 1024;

pub(crate) fn post_bounded_json(
    host: &str,
    port: u16,
    path: &str,
    body: &str,
    timeout: Duration,
    mut cancel_requested: impl FnMut() -> Result<bool, AppError>,
) -> Result<String, AppError> {
    let started_at = Instant::now();
    if cancel_requested()? {
        return Err(AppError::blocked("backend preflight가 취소되었습니다."));
    }
    let address = format!("{host}:{port}");
    let mut addresses = address.to_socket_addrs().map_err(|err| {
        AppError::runtime(format!("backend address resolve 실패: {address} ({err})"))
    })?;
    let socket_addr = addresses
        .next()
        .ok_or_else(|| AppError::runtime(format!("backend address 없음: {address}")))?;
    let connect_timeout = remaining_timeout(started_at, timeout)
        .ok_or_else(|| AppError::blocked("backend preflight timeout"))?;
    let mut stream = TcpStream::connect_timeout(&socket_addr, connect_timeout).map_err(|err| {
        if matches!(err.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) {
            AppError::blocked("backend preflight 연결 timeout")
        } else {
            AppError::runtime(format!(
                "backend preflight 연결 실패: {socket_addr} ({err})"
            ))
        }
    })?;
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {host}:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nAccept: application/json\r\nConnection: close\r\n\r\n{}",
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
        return Err(termination_error(termination));
    }

    read_response(
        &mut stream,
        path,
        started_at,
        timeout,
        &mut cancel_requested,
    )
}

fn read_response(
    stream: &mut TcpStream,
    path: &str,
    started_at: Instant,
    timeout: Duration,
    cancel_requested: &mut impl FnMut() -> Result<bool, AppError>,
) -> Result<String, AppError> {
    let mut http = HttpResponseDecoder::default();
    let mut response = Vec::new();
    let mut read_buffer = [0_u8; 16 * 1024];
    loop {
        if cancel_requested()? {
            let _ = stream.shutdown(Shutdown::Both);
            return Err(AppError::blocked("backend preflight가 취소되었습니다."));
        }
        if started_at.elapsed() >= timeout {
            let _ = stream.shutdown(Shutdown::Both);
            return Err(AppError::blocked("backend preflight timeout"));
        }
        let remaining = remaining_timeout(started_at, timeout)
            .ok_or_else(|| AppError::blocked("backend preflight timeout"))?;
        stream
            .set_read_timeout(Some(READ_POLL_INTERVAL.min(remaining)))
            .map_err(|err| {
                AppError::runtime(format!("backend preflight read timeout 설정 실패: {err}"))
            })?;
        match stream.read(&mut read_buffer) {
            Ok(0) => return decode_response(response),
            Ok(read_bytes) => {
                for body_chunk in http.push(&read_buffer[..read_bytes])? {
                    if response.len().saturating_add(body_chunk.len()) > MAX_RESPONSE_BYTES {
                        return Err(AppError::blocked(
                            "backend preflight response가 허용 크기를 초과했습니다.",
                        ));
                    }
                    response.extend_from_slice(&body_chunk);
                }
                if let Some(status_code) = http.failed_status_code() {
                    return Err(AppError::blocked(format!(
                        "backend capability mismatch\n- endpoint: {path}\n- status code: {status_code}\n- 동작: response body는 기록하지 않았습니다."
                    )));
                }
                if http.body_complete {
                    return decode_response(response);
                }
            }
            Err(err) if matches!(err.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(err) => {
                return Err(AppError::runtime(format!(
                    "backend preflight response read 실패: {err}"
                )));
            }
        }
    }
}

fn decode_response(response: Vec<u8>) -> Result<String, AppError> {
    String::from_utf8(response)
        .map_err(|_| AppError::blocked("backend preflight response가 유효한 UTF-8이 아닙니다."))
}

fn termination_error(termination: StreamTermination) -> AppError {
    match termination {
        StreamTermination::Cancelled => AppError::blocked("backend preflight가 취소되었습니다."),
        StreamTermination::TimedOut => AppError::blocked("backend preflight timeout"),
        StreamTermination::Completed => {
            AppError::runtime("backend preflight request가 완료되지 않았습니다.")
        }
    }
}

#[cfg(test)]
mod tests;
