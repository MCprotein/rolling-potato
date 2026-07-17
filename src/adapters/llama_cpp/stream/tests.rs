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
        let response = format!("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{size}\r\n");
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
    let event = format!("data: {{\"choices\":[{{\"delta\":{{\"content\":\"{chunk}\"}}}}]}}\n\n");
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
                ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted | ErrorKind::BrokenPipe
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
