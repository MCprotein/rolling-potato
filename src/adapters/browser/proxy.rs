use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::adapters::web_search::{resolve_public_browser_target, validate_browser_navigation_url};
use crate::foundation::error::AppError;

const ACCEPT_POLL: Duration = Duration::from_millis(20);
const IO_POLL: Duration = Duration::from_millis(250);
const CONNECT_BUDGET: Duration = Duration::from_secs(5);
const MAX_CONNECT_ATTEMPTS: usize = 4;
const MAX_HEADER_BYTES: usize = 16 * 1024;
const MAX_ACTIVE_TUNNELS: usize = 16;

pub(super) struct PublicHttpsProxy {
    address: SocketAddr,
    shutdown: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl PublicHttpsProxy {
    pub(super) fn start() -> Result<Self, AppError> {
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|error| {
            AppError::runtime(format!(
                "격리 브라우저 public HTTPS proxy를 시작하지 못했습니다: {error}"
            ))
        })?;
        listener.set_nonblocking(true).map_err(|error| {
            AppError::runtime(format!(
                "격리 브라우저 proxy accept mode를 설정하지 못했습니다: {error}"
            ))
        })?;
        let address = listener.local_addr().map_err(|error| {
            AppError::runtime(format!(
                "격리 브라우저 proxy 주소를 확인하지 못했습니다: {error}"
            ))
        })?;
        let shutdown = Arc::new(AtomicBool::new(false));
        let active = Arc::new(AtomicUsize::new(0));
        let thread_shutdown = Arc::clone(&shutdown);
        let thread = thread::Builder::new()
            .name("rpotato-browser-proxy".to_string())
            .spawn(move || accept_loop(listener, thread_shutdown, active))
            .map_err(|error| {
                AppError::runtime(format!(
                    "격리 브라우저 proxy worker를 시작하지 못했습니다: {error}"
                ))
            })?;
        Ok(Self {
            address,
            shutdown,
            thread: Some(thread),
        })
    }

    pub(super) fn address(&self) -> SocketAddr {
        self.address
    }
}

impl Drop for PublicHttpsProxy {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        let _ = TcpStream::connect_timeout(&self.address, IO_POLL);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn accept_loop(listener: TcpListener, shutdown: Arc<AtomicBool>, active: Arc<AtomicUsize>) {
    let mut workers = Vec::new();
    while !shutdown.load(Ordering::Acquire) {
        reap_finished_workers(&mut workers);
        match listener.accept() {
            Ok((stream, _)) => {
                if active.fetch_add(1, Ordering::AcqRel) >= MAX_ACTIVE_TUNNELS {
                    active.fetch_sub(1, Ordering::AcqRel);
                    reject(stream, "503 Service Unavailable");
                    continue;
                }
                let worker_shutdown = Arc::clone(&shutdown);
                let worker_active = Arc::clone(&active);
                workers.push(thread::spawn(move || {
                    let _guard = ActiveTunnelGuard(worker_active);
                    handle_client(stream, worker_shutdown);
                }));
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(ACCEPT_POLL);
            }
            Err(_) => break,
        }
    }
    for worker in workers {
        let _ = worker.join();
    }
}

fn reap_finished_workers(workers: &mut Vec<JoinHandle<()>>) {
    let mut index = 0;
    while index < workers.len() {
        if workers[index].is_finished() {
            let _ = workers.swap_remove(index).join();
        } else {
            index += 1;
        }
    }
}

struct ActiveTunnelGuard(Arc<AtomicUsize>);

impl Drop for ActiveTunnelGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

fn handle_client(mut client: TcpStream, shutdown: Arc<AtomicBool>) {
    let _ = client.set_read_timeout(Some(IO_POLL));
    let _ = client.set_write_timeout(Some(IO_POLL));
    let header = match read_header(&mut client, &shutdown) {
        Ok(header) => header,
        Err(_) => {
            reject(client, "400 Bad Request");
            return;
        }
    };
    let authority = match connect_authority(&header) {
        Ok(authority) => authority,
        Err(_) => {
            reject(client, "403 Forbidden");
            return;
        }
    };
    let addresses = match resolve_public_browser_target(&authority.host, authority.port) {
        Ok(addresses) => addresses,
        Err(_) => {
            reject(client, "403 Forbidden");
            return;
        }
    };
    let Some(server) = connect_public_target(&addresses) else {
        reject(client, "502 Bad Gateway");
        return;
    };
    if client
        .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
        .is_err()
    {
        return;
    }
    relay_tunnel(client, server, shutdown);
}

fn read_header(stream: &mut TcpStream, shutdown: &AtomicBool) -> io::Result<String> {
    let mut header = Vec::new();
    let mut byte = [0_u8; 1];
    while !header.ends_with(b"\r\n\r\n") {
        if shutdown.load(Ordering::Acquire) || header.len() >= MAX_HEADER_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "proxy header rejected",
            ));
        }
        match stream.read(&mut byte) {
            Ok(0) => return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "proxy eof")),
            Ok(_) => header.push(byte[0]),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                continue;
            }
            Err(error) => return Err(error),
        }
    }
    String::from_utf8(header)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "proxy header utf8"))
}

struct ConnectAuthority {
    host: String,
    port: u16,
}

fn connect_authority(header: &str) -> Result<ConnectAuthority, AppError> {
    let request_line = header.lines().next().unwrap_or_default();
    let mut fields = request_line.split_whitespace();
    if fields.next() != Some("CONNECT") {
        return Err(AppError::blocked(
            "격리 브라우저 proxy는 HTTPS CONNECT만 허용합니다.",
        ));
    }
    let authority = fields
        .next()
        .ok_or_else(|| AppError::blocked("격리 브라우저 proxy authority가 없습니다."))?;
    if !matches!(fields.next(), Some("HTTP/1.1") | Some("HTTP/1.0"))
        || fields.next().is_some()
        || authority.contains('@')
    {
        return Err(AppError::blocked(
            "격리 브라우저 proxy request 형식이 허용되지 않습니다.",
        ));
    }
    let (host, port) = split_authority(authority)?;
    validate_browser_navigation_url(&format!("https://{authority}/"))?;
    Ok(ConnectAuthority { host, port })
}

fn split_authority(authority: &str) -> Result<(String, u16), AppError> {
    let (host, port) = if let Some(rest) = authority.strip_prefix('[') {
        let (host, port) = rest
            .split_once("]:")
            .ok_or_else(|| AppError::blocked("IPv6 proxy authority 형식이 올바르지 않습니다."))?;
        (host, port)
    } else {
        let (host, port) = authority
            .rsplit_once(':')
            .ok_or_else(|| AppError::blocked("proxy authority에 port가 없습니다."))?;
        if host.contains(':') {
            return Err(AppError::blocked(
                "IPv6 proxy authority에는 대괄호가 필요합니다.",
            ));
        }
        (host, port)
    };
    let port = port
        .parse::<u16>()
        .ok()
        .filter(|port| *port == 443)
        .ok_or_else(|| AppError::blocked("격리 브라우저 proxy는 port 443만 허용합니다."))?;
    if host.is_empty() || host.len() > 253 {
        return Err(AppError::blocked(
            "격리 브라우저 proxy host 길이가 올바르지 않습니다.",
        ));
    }
    Ok((host.to_string(), port))
}

fn connect_public_target(addresses: &[SocketAddr]) -> Option<TcpStream> {
    let deadline = Instant::now() + CONNECT_BUDGET;
    for address in addresses.iter().take(MAX_CONNECT_ATTEMPTS) {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return None;
        }
        if let Ok(stream) =
            TcpStream::connect_timeout(address, remaining.min(Duration::from_secs(2)))
        {
            let _ = stream.set_read_timeout(Some(IO_POLL));
            let _ = stream.set_write_timeout(Some(IO_POLL));
            let _ = stream.set_nodelay(true);
            return Some(stream);
        }
    }
    None
}

fn relay_tunnel(client: TcpStream, server: TcpStream, shutdown: Arc<AtomicBool>) {
    let Ok(mut client_read) = client.try_clone() else {
        return;
    };
    let Ok(mut server_write) = server.try_clone() else {
        return;
    };
    thread::scope(|scope| {
        let outbound_shutdown = Arc::clone(&shutdown);
        scope.spawn(move || {
            relay(&mut client_read, &mut server_write, &outbound_shutdown);
        });
        let mut server_read = server;
        let mut client_write = client;
        relay(&mut server_read, &mut client_write, &shutdown);
    });
}

fn relay(input: &mut TcpStream, output: &mut TcpStream, shutdown: &AtomicBool) {
    let mut buffer = [0_u8; 16 * 1024];
    while !shutdown.load(Ordering::Acquire) {
        match input.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                if output.write_all(&buffer[..read]).is_err() {
                    break;
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                continue;
            }
            Err(_) => break,
        }
    }
    let _ = output.shutdown(Shutdown::Write);
}

fn reject(mut stream: TcpStream, status: &str) {
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_contract_accepts_only_https_443_without_credentials() {
        let parsed =
            connect_authority("CONNECT www.google.com:443 HTTP/1.1\r\nHost: ignored\r\n\r\n")
                .unwrap();
        assert_eq!(parsed.host, "www.google.com");
        assert_eq!(parsed.port, 443);
        for invalid in [
            "GET https://www.google.com/ HTTP/1.1\r\n\r\n",
            "CONNECT www.google.com:80 HTTP/1.1\r\n\r\n",
            "CONNECT user@www.google.com:443 HTTP/1.1\r\n\r\n",
            "CONNECT localhost:443 HTTP/1.1\r\n\r\n",
            "CONNECT 127.0.0.1:443 HTTP/1.1\r\n\r\n",
        ] {
            assert!(connect_authority(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn running_proxy_rejects_loopback_without_opening_a_tunnel() {
        let proxy = PublicHttpsProxy::start().unwrap();
        let mut client = TcpStream::connect(proxy.address()).unwrap();
        client
            .write_all(b"CONNECT 127.0.0.1:443 HTTP/1.1\r\n\r\n")
            .unwrap();
        let mut response = String::new();
        client.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 403 Forbidden"));
    }

    #[test]
    fn proxy_shutdown_interrupts_an_idle_client_and_joins_workers() {
        let proxy = PublicHttpsProxy::start().unwrap();
        let _idle_client = TcpStream::connect(proxy.address()).unwrap();
        drop(proxy);
    }
}
