//! Bounded HTTP health probing.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use super::HealthProbe;

pub(crate) fn probe_health(host: &str, port: u16, timeout: Duration) -> HealthProbe {
    let address = format!("{host}:{port}");
    let Ok(mut addresses) = address.to_socket_addrs() else {
        return HealthProbe {
            status: "unreachable",
            tcp_connected: false,
            http_status_line: None,
            error: Some(format!("address resolve 실패: {address}")),
        };
    };
    let Some(socket_addr) = addresses.next() else {
        return HealthProbe {
            status: "unreachable",
            tcp_connected: false,
            http_status_line: None,
            error: Some(format!("address 없음: {address}")),
        };
    };

    let Ok(mut stream) = TcpStream::connect_timeout(&socket_addr, timeout) else {
        return HealthProbe {
            status: "unreachable",
            tcp_connected: false,
            http_status_line: None,
            error: Some(format!("connect 실패: {socket_addr}")),
        };
    };

    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(timeout));
    let request =
        format!("GET /health HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: close\r\n\r\n");
    if let Err(err) = stream.write_all(request.as_bytes()) {
        return HealthProbe {
            status: "unhealthy",
            tcp_connected: true,
            http_status_line: None,
            error: Some(format!("health request write 실패: {err}")),
        };
    }

    let mut response = Vec::with_capacity(256);
    let status_line = loop {
        if let Some(status_line) = first_http_status_line(&response) {
            break status_line;
        }
        if response.len() >= 8 * 1024 {
            return HealthProbe {
                status: "unhealthy",
                tcp_connected: true,
                http_status_line: None,
                error: Some("health response status line이 8 KiB를 초과했습니다.".to_string()),
            };
        }
        let mut buffer = [0_u8; 256];
        match stream.read(&mut buffer) {
            Ok(0) => {
                return HealthProbe {
                    status: "unhealthy",
                    tcp_connected: true,
                    http_status_line: None,
                    error: Some("health response가 status line 전에 종료됐습니다.".to_string()),
                };
            }
            Ok(read) => response.extend_from_slice(&buffer[..read]),
            Err(err) => {
                return HealthProbe {
                    status: "unhealthy",
                    tcp_connected: true,
                    http_status_line: None,
                    error: Some(format!("health response read 실패: {err}")),
                };
            }
        }
    };
    let status = if status_line.contains(" 200 ") || status_line.ends_with(" 200") {
        "healthy"
    } else {
        "unhealthy"
    };

    HealthProbe {
        status,
        tcp_connected: true,
        http_status_line: Some(status_line),
        error: None,
    }
}

pub(crate) fn first_http_status_line(response: &[u8]) -> Option<String> {
    let end = response.iter().position(|byte| *byte == b'\n')?;
    let line = response[..end]
        .strip_suffix(b"\r")
        .unwrap_or(&response[..end]);
    std::str::from_utf8(line).ok().map(str::to_string)
}
