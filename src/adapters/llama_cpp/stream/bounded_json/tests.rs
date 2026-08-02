use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

use super::post_bounded_json;

#[test]
fn posts_bounded_json_and_collects_chunked_input_token_response() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        read_http_request(&mut socket);
        let body = r#"{"object":"response.input_tokens","input_tokens":777}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\n\r\n{:X}\r\n{}\r\n0\r\n\r\n",
            body.len(),
            body
        );
        socket.write_all(response.as_bytes()).unwrap();
    });

    let body = post_bounded_json(
        "127.0.0.1",
        port,
        "/v1/chat/completions/input_tokens",
        r#"{"messages":[],"max_tokens":1}"#,
        Duration::from_secs(2),
        || Ok(false),
    )
    .unwrap();
    server.join().unwrap();

    assert_eq!(
        body,
        r#"{"object":"response.input_tokens","input_tokens":777}"#
    );
}

#[test]
fn bounded_json_http_error_does_not_expose_response_body() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        read_http_request(&mut socket);
        socket
            .write_all(
                b"HTTP/1.1 404 Not Found\r\nContent-Length: 18\r\nConnection: close\r\n\r\nsecret-image-data",
            )
            .unwrap();
    });

    let error = post_bounded_json(
        "127.0.0.1",
        port,
        "/v1/chat/completions/input_tokens",
        "{}",
        Duration::from_secs(2),
        || Ok(false),
    )
    .unwrap_err();
    server.join().unwrap();

    assert!(error.message.contains("capability mismatch"));
    assert!(error.message.contains("404"));
    assert!(!error.message.contains("secret-image-data"));
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
    assert!(String::from_utf8_lossy(&request).contains("Accept: application/json"));
}
