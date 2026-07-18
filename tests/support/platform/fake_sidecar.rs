use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
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

    let request_body = &request[header_end..];
    if let Ok(path) = std::env::var("RPOTATO_FAKE_REQUEST_REQUIRED_SENTINEL_FILE") {
        if let Ok(required_sentinel) = std::fs::read_to_string(path) {
            if !required_sentinel.is_empty()
                && find_bytes(request_body, required_sentinel.as_bytes()).is_none()
            {
                eprintln!("fake sidecar request sentinel missing");
                return;
            }
        }
    }

    if let Ok(path) = std::env::var("RPOTATO_FAKE_REQUEST_MARKER") {
        if let Ok(mut marker) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = marker.write_all(b"request\n");
        }
    }
    if let Ok(path) = std::env::var("RPOTATO_FAKE_REQUEST_SIZE_MARKER") {
        if let Ok(mut marker) = OpenOptions::new().create(true).append(true).open(path) {
            let body_bytes = request.len().saturating_sub(header_end);
            let _ = writeln!(marker, "{body_bytes}");
        }
    }

    if let Ok(path) = std::env::var("RPOTATO_FAKE_RESPONSE_FILE") {
        let content = match std::fs::read_to_string(&path) {
            Ok(content) => content,
            Err(error) => {
                eprintln!("fake sidecar response read failed: {path}: {error}");
                return;
            }
        };
        let request_body = String::from_utf8_lossy(request_body);
        let content = expand_fixture_template(content, &request_body);
        let body = format!(
            "data: {{\"choices\":[{{\"delta\":{{\"content\":{}}},\"finish_reason\":\"stop\"}}]}}\n\ndata: {{\"choices\":[],\"usage\":{{\"prompt_tokens\":10,\"completion_tokens\":10,\"total_tokens\":20}}}}\n\ndata: [DONE]\n\n",
            json_string(&content)
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        if let Err(error) = stream
            .write_all(response.as_bytes())
            .and_then(|_| stream.write_all(body.as_bytes()))
            .and_then(|_| stream.flush())
        {
            eprintln!("fake sidecar fixture response write failed: {error}");
            return;
        }
        let _ = stream.shutdown(Shutdown::Write);
        let mut drain = [0_u8; 256];
        while matches!(stream.read(&mut drain), Ok(read) if read > 0) {}
        return;
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

fn json_string(value: &str) -> String {
    let mut escaped = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character if character <= '\u{001f}' => {
                escaped.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => escaped.push(character),
        }
    }
    escaped.push('"');
    escaped
}

fn expand_fixture_template(mut content: String, request_body: &str) -> String {
    for (placeholder, marker) in [
        ("{{SUBAGENT_ID}}", "subagent_id="),
        ("{{PARENT_WORKFLOW_ID}}", "parent_workflow_id="),
    ] {
        if !content.contains(placeholder) {
            continue;
        }
        let Some(value) = token_after(request_body, marker) else {
            eprintln!("fake sidecar template marker missing: {marker}");
            return content;
        };
        content = content.replace(placeholder, value);
    }
    content
}

fn token_after<'a>(input: &'a str, marker: &str) -> Option<&'a str> {
    let rest = input.split_once(marker)?.1;
    let length = rest
        .bytes()
        .take_while(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_' | b'.')
        })
        .count();
    (length > 0).then(|| &rest[..length])
}
