use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    let host = arg_value(&args, "--host").unwrap_or("127.0.0.1");
    let port = arg_value(&args, "--port")
        .and_then(|value| value.parse::<u16>().ok())
        .expect("--port is required");
    let listener = TcpListener::bind((host, port)).expect("bind fake sidecar");
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                thread::spawn(move || handle(stream));
            }
            Err(_) => break,
        }
    }
}

fn arg_value<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|arg| arg == name)
        .and_then(|index| args.get(index + 1))
        .map(String::as_str)
}

fn handle(mut stream: TcpStream) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut request = Vec::new();
    let mut buffer = [0_u8; 4096];
    let header_end = loop {
        let Ok(read) = stream.read(&mut buffer) else {
            return;
        };
        if read == 0 {
            return;
        }
        request.extend_from_slice(&buffer[..read]);
        if let Some(index) = find_bytes(&request, b"\r\n\r\n") {
            break index + 4;
        }
    };
    let (is_get, content_length) = {
        let headers = String::from_utf8_lossy(&request[..header_end]);
        let content_length = headers
            .lines()
            .find_map(|line| line.split_once(':'))
            .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
            .and_then(|(_, value)| value.trim().parse::<usize>().ok())
            .unwrap_or(0);
        (headers.starts_with("GET "), content_length)
    };
    while request.len().saturating_sub(header_end) < content_length {
        let Ok(read) = stream.read(&mut buffer) else {
            return;
        };
        if read == 0 {
            return;
        }
        request.extend_from_slice(&buffer[..read]);
    }

    if is_get {
        let body = b"{\"status\":\"ok\"}";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.write_all(body);
        return;
    }

    if let Ok(path) = std::env::var("RPOTATO_FAKE_REQUEST_MARKER") {
        if let Ok(mut marker) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = marker.write_all(b"request\n");
        }
    }

    if stream
        .write_all(
            b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n",
        )
        .is_err()
    {
        return;
    }
    loop {
        if stream.write_all(b": keepalive\n\n").is_err() || stream.flush().is_err() {
            return;
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}
