use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::process::Command;
#[cfg(unix)]
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use super::discovery::{test_platform_candidates, BrowserExecutable, BrowserKind};
use super::protocol::{test_endpoint, CdpCommand, CdpMethod, RestrictedCdpClient};
use super::session::{launch_test_session, BrowserSessionOptions};
use super::websocket::test_websocket_accept;

#[cfg(unix)]
static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[test]
fn platform_discovery_contract_covers_macos_linux_and_windows() {
    let path_entries = vec![PathBuf::from("/custom/bin")];
    let mac = test_platform_candidates(
        "macos",
        Some(Path::new("/Users/example")),
        None,
        None,
        &path_entries,
    );
    assert_eq!(
        mac.iter()
            .take(3)
            .map(|candidate| candidate.path.to_string_lossy().into_owned())
            .collect::<Vec<_>>(),
        vec![
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Chromium.app/Contents/MacOS/Chromium",
            "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
        ]
    );
    assert!(mac.iter().any(|candidate| {
        candidate.kind == BrowserKind::Chromium
            && candidate.path.as_path() == Path::new("/custom/bin/chromium")
    }));

    let linux = test_platform_candidates("linux", None, None, None, &path_entries);
    assert!(linux.iter().any(|candidate| {
        candidate.kind == BrowserKind::Chrome
            && candidate.path.as_path() == Path::new("/custom/bin/google-chrome")
    }));
    assert!(linux.iter().any(|candidate| {
        candidate.kind == BrowserKind::Edge
            && candidate.path.as_path() == Path::new("/custom/bin/microsoft-edge")
    }));

    let windows = test_platform_candidates(
        "windows",
        None,
        Some(Path::new("C:/Users/example/AppData/Local")),
        Some(Path::new("C:/Program Files")),
        &[PathBuf::from("C:/bin")],
    );
    assert!(windows.iter().any(|candidate| {
        candidate.kind == BrowserKind::Chrome
            && candidate
                .path
                .ends_with("Google/Chrome/Application/chrome.exe")
    }));
    assert!(windows.iter().any(|candidate| {
        candidate.kind == BrowserKind::Edge
            && candidate.path.as_path() == Path::new("C:/bin/msedge.exe")
    }));
}

#[test]
fn active_port_file_accepts_only_a_local_browser_endpoint() {
    let endpoint = test_endpoint("49152\n/devtools/browser/abc-123_DEF\n").unwrap();
    assert_eq!(
        endpoint.display_url(),
        "ws://127.0.0.1:49152/devtools/browser/abc-123_DEF"
    );
    for invalid in [
        "0\n/devtools/browser/id\n",
        "9222\n/devtools/page/id\n",
        "9222\n/devtools/browser/\n",
        "9222\n/devtools/browser/id?token=secret\n",
        "9222\n/devtools/browser/id\nunexpected\n",
        "example.com\n/devtools/browser/id\n",
    ] {
        assert!(test_endpoint(invalid).is_err(), "accepted: {invalid:?}");
    }
}

#[test]
fn websocket_accept_matches_the_rfc6455_vector() {
    assert_eq!(
        test_websocket_accept("dGhlIHNhbXBsZSBub25jZQ=="),
        "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
    );
}

#[test]
fn restricted_protocol_round_trip_masks_commands_and_matches_response_ids() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        accept_websocket(&mut stream);
        let command = read_masked_text(&mut stream);
        assert_eq!(
            command,
            "{\"id\":7,\"method\":\"Page.enable\",\"params\":{}}"
        );
        write_server_text(&mut stream, "{\"method\":\"Page.lifecycleEvent\"}");
        write_server_text(&mut stream, "{\"id\":7,\"result\":{}}");
    });

    let endpoint = test_endpoint(&format!("{port}\n/devtools/browser/test\n")).unwrap();
    let mut client = RestrictedCdpClient::connect(&endpoint, Duration::from_secs(1)).unwrap();
    let command = CdpCommand::new(7, CdpMethod::PageEnable, "{}").unwrap();
    let response = client.send_command(&command).unwrap();
    assert_eq!(response.raw_json, "{\"id\":7,\"result\":{}}");
    server.join().unwrap();
}

#[test]
fn restricted_protocol_scopes_target_commands_to_an_attached_session() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        accept_websocket(&mut stream);
        let command = read_masked_text(&mut stream);
        assert_eq!(
            command,
            "{\"id\":8,\"method\":\"Page.navigate\",\"params\":{\"url\":\"https://example.com/\"},\"sessionId\":\"session-1\"}"
        );
        write_server_text(
            &mut stream,
            "{\"id\":8,\"result\":{\"frameId\":\"frame-1\"}}",
        );
    });

    let endpoint = test_endpoint(&format!("{port}\n/devtools/browser/test\n")).unwrap();
    let mut client = RestrictedCdpClient::connect(&endpoint, Duration::from_secs(1)).unwrap();
    let command = CdpCommand::new(
        8,
        CdpMethod::PageNavigate,
        "{\"url\":\"https://example.com/\"}",
    )
    .unwrap()
    .with_session_id("session-1")
    .unwrap();
    let response = client.send_command(&command).unwrap();
    assert_eq!(
        response.raw_json,
        "{\"id\":8,\"result\":{\"frameId\":\"frame-1\"}}"
    );
    server.join().unwrap();
}

#[test]
fn protocol_handshake_timeout_is_bounded() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = thread::spawn(move || {
        let (_stream, _) = listener.accept().unwrap();
        thread::sleep(Duration::from_millis(150));
    });
    let endpoint = test_endpoint(&format!("{port}\n/devtools/browser/test\n")).unwrap();
    let started = Instant::now();

    assert!(RestrictedCdpClient::connect(&endpoint, Duration::from_millis(30)).is_err());
    assert!(started.elapsed() < Duration::from_secs(1));
    server.join().unwrap();
}

#[test]
fn oversized_protocol_frame_is_rejected_before_allocating_its_payload() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        accept_websocket(&mut stream);
        let _ = read_masked_text(&mut stream);
        stream.write_all(&[0x81, 127]).unwrap();
        stream
            .write_all(&(1024_u64 * 1024 + 1).to_be_bytes())
            .unwrap();
    });

    let endpoint = test_endpoint(&format!("{port}\n/devtools/browser/test\n")).unwrap();
    let mut client = RestrictedCdpClient::connect(&endpoint, Duration::from_secs(1)).unwrap();
    let command = CdpCommand::new(9, CdpMethod::PageEnable, "{}").unwrap();
    let error = client.send_command(&command).unwrap_err();
    assert!(error.message.contains("허용 크기"));
    server.join().unwrap();
}

#[cfg(unix)]
#[test]
fn isolated_browser_session_cleans_up_process_group_and_profile() {
    use std::os::unix::fs::PermissionsExt;

    let root = test_root("session-cleanup");
    fs::create_dir_all(&root).unwrap();
    let script = root.join("fake-browser.sh");
    fs::write(
        &script,
        "#!/bin/sh\nprofile=''\nfor arg in \"$@\"; do\n  case \"$arg\" in\n    --user-data-dir=*) profile=${arg#--user-data-dir=} ;;\n  esac\ndone\nprintf '49153\\n/devtools/browser/fake-session\\n' > \"$profile/DevToolsActivePort\"\nwhile :; do sleep 1; done\n",
    )
    .unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
    let executable = BrowserExecutable {
        kind: BrowserKind::Chromium,
        path: script,
    };
    let session = launch_test_session(
        &executable,
        BrowserSessionOptions {
            headless: true,
            startup_timeout: Duration::from_secs(2),
            ..BrowserSessionOptions::default()
        },
        &root,
    )
    .unwrap();
    let pid = session.process_id().unwrap();
    let profile = session.profile_dir().unwrap().to_path_buf();
    assert!(profile.join("DevToolsActivePort").is_file());

    session.close();

    assert!(!profile.exists());
    assert!(!process_is_alive(pid));
    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn failed_browser_startup_removes_the_temporary_profile() {
    use std::os::unix::fs::PermissionsExt;

    let root = test_root("startup-failure");
    fs::create_dir_all(&root).unwrap();
    let script = root.join("fake-browser.sh");
    fs::write(&script, "#!/bin/sh\nexit 17\n").unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
    let executable = BrowserExecutable {
        kind: BrowserKind::Chromium,
        path: script.clone(),
    };
    assert!(launch_test_session(
        &executable,
        BrowserSessionOptions {
            headless: true,
            startup_timeout: Duration::from_millis(250),
            ..BrowserSessionOptions::default()
        },
        &root,
    )
    .is_err());
    assert_eq!(
        fs::read_dir(&root)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>(),
        vec![script]
    );
    let _ = fs::remove_dir_all(root);
}

fn accept_websocket(stream: &mut TcpStream) {
    let headers = read_headers(stream);
    let key = headers
        .lines()
        .find_map(|line| {
            line.split_once(':').and_then(|(name, value)| {
                name.eq_ignore_ascii_case("sec-websocket-key")
                    .then(|| value.trim().to_string())
            })
        })
        .expect("websocket key");
    let accept = test_websocket_accept(&key);
    write!(
        stream,
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"
    )
    .unwrap();
}

fn read_headers(stream: &mut TcpStream) -> String {
    let mut bytes = Vec::new();
    let mut byte = [0_u8; 1];
    while !bytes.ends_with(b"\r\n\r\n") {
        stream.read_exact(&mut byte).unwrap();
        bytes.push(byte[0]);
        assert!(bytes.len() < 16 * 1024);
    }
    String::from_utf8(bytes).unwrap()
}

fn read_masked_text(stream: &mut TcpStream) -> String {
    let mut header = [0_u8; 2];
    stream.read_exact(&mut header).unwrap();
    assert_eq!(header[0], 0x81);
    assert_ne!(header[1] & 0x80, 0);
    let length = match header[1] & 0x7f {
        length @ 0..=125 => usize::from(length),
        126 => {
            let mut bytes = [0_u8; 2];
            stream.read_exact(&mut bytes).unwrap();
            usize::from(u16::from_be_bytes(bytes))
        }
        marker => panic!("unexpected length marker: {marker}"),
    };
    let mut mask = [0_u8; 4];
    stream.read_exact(&mut mask).unwrap();
    let mut payload = vec![0_u8; length];
    stream.read_exact(&mut payload).unwrap();
    for (index, byte) in payload.iter_mut().enumerate() {
        *byte ^= mask[index % mask.len()];
    }
    String::from_utf8(payload).unwrap()
}

fn write_server_text(stream: &mut TcpStream, message: &str) {
    assert!(message.len() <= 125);
    stream.write_all(&[0x81, message.len() as u8]).unwrap();
    stream.write_all(message.as_bytes()).unwrap();
}

#[cfg(unix)]
fn process_is_alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(unix)]
fn test_root(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "rpotato-browser-{name}-{}-{}",
        std::process::id(),
        TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ))
}
