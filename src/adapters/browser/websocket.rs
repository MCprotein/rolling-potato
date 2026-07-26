use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use crate::foundation::error::AppError;

const MAX_HANDSHAKE_BYTES: usize = 16 * 1024;
const MAX_FRAME_BYTES: usize = 1024 * 1024;
const MAX_CONTROL_PAYLOAD_BYTES: usize = 125;
static NONCE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

pub(super) struct LocalWebSocket {
    stream: TcpStream,
    pending: Vec<u8>,
}

impl LocalWebSocket {
    pub(super) fn connect(
        address: SocketAddr,
        path: &str,
        timeout: Duration,
    ) -> Result<Self, AppError> {
        if !address.ip().is_loopback() {
            return Err(AppError::blocked(
                "CDP WebSocket은 loopback address에만 연결할 수 있습니다.",
            ));
        }
        let mut stream = TcpStream::connect_timeout(&address, timeout)
            .map_err(|_| AppError::runtime("격리 브라우저 CDP endpoint에 연결하지 못했습니다."))?;
        stream
            .set_read_timeout(Some(timeout))
            .and_then(|()| stream.set_write_timeout(Some(timeout)))
            .map_err(|_| AppError::runtime("CDP socket timeout을 설정하지 못했습니다."))?;

        let key = websocket_key();
        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n",
            address.port()
        );
        stream
            .write_all(request.as_bytes())
            .map_err(|_| AppError::runtime("CDP WebSocket handshake를 보내지 못했습니다."))?;
        let (headers, pending) = read_handshake(&mut stream)?;
        validate_handshake(&headers, &key)?;
        Ok(Self { stream, pending })
    }

    pub(super) fn send_text(&mut self, text: &str) -> Result<(), AppError> {
        if text.len() > MAX_FRAME_BYTES {
            return Err(AppError::blocked(
                "CDP WebSocket command frame이 허용 크기를 초과했습니다.",
            ));
        }
        self.send_frame(0x1, text.as_bytes())
    }

    pub(super) fn read_text(&mut self) -> Result<String, AppError> {
        for _ in 0..16 {
            let (opcode, payload) = self.read_frame()?;
            match opcode {
                0x1 => {
                    return String::from_utf8(payload).map_err(|_| {
                        AppError::blocked("CDP WebSocket text frame이 UTF-8이 아닙니다.")
                    });
                }
                0x8 => {
                    return Err(AppError::runtime(
                        "격리 브라우저가 CDP WebSocket 연결을 종료했습니다.",
                    ));
                }
                0x9 => {
                    self.send_frame(0xA, &payload)?;
                }
                0xA => {}
                _ => {
                    return Err(AppError::blocked(
                        "CDP WebSocket이 허용하지 않은 frame opcode를 반환했습니다.",
                    ));
                }
            }
        }
        Err(AppError::runtime(
            "CDP WebSocket control frame 수가 제한을 초과했습니다.",
        ))
    }

    fn send_frame(&mut self, opcode: u8, payload: &[u8]) -> Result<(), AppError> {
        if matches!(opcode, 0x8..=0xA) && payload.len() > MAX_CONTROL_PAYLOAD_BYTES {
            return Err(AppError::blocked(
                "CDP WebSocket control frame이 허용 크기를 초과했습니다.",
            ));
        }
        let mask = masking_key();
        let mut frame = Vec::with_capacity(payload.len() + 14);
        frame.push(0x80 | opcode);
        match payload.len() {
            length @ 0..=125 => frame.push(0x80 | length as u8),
            length @ 126..=65_535 => {
                frame.push(0x80 | 126);
                frame.extend_from_slice(&(length as u16).to_be_bytes());
            }
            length => {
                frame.push(0x80 | 127);
                frame.extend_from_slice(&(length as u64).to_be_bytes());
            }
        }
        frame.extend_from_slice(&mask);
        frame.extend(
            payload
                .iter()
                .enumerate()
                .map(|(index, byte)| byte ^ mask[index % mask.len()]),
        );
        self.stream
            .write_all(&frame)
            .map_err(|_| AppError::runtime("CDP WebSocket frame을 보내지 못했습니다."))
    }

    fn read_frame(&mut self) -> Result<(u8, Vec<u8>), AppError> {
        let mut header = [0_u8; 2];
        self.read_exact(&mut header)?;
        if header[0] & 0x80 == 0 {
            return Err(AppError::blocked(
                "CDP WebSocket fragmented frame은 허용하지 않습니다.",
            ));
        }
        let opcode = header[0] & 0x0f;
        if header[1] & 0x80 != 0 {
            return Err(AppError::blocked(
                "CDP WebSocket server frame은 masked 형식일 수 없습니다.",
            ));
        }
        let length = match header[1] & 0x7f {
            length @ 0..=125 => usize::from(length),
            126 => {
                let mut bytes = [0_u8; 2];
                self.read_exact(&mut bytes)?;
                usize::from(u16::from_be_bytes(bytes))
            }
            127 => {
                let mut bytes = [0_u8; 8];
                self.read_exact(&mut bytes)?;
                usize::try_from(u64::from_be_bytes(bytes)).map_err(|_| {
                    AppError::blocked("CDP WebSocket frame 길이를 처리할 수 없습니다.")
                })?
            }
            _ => unreachable!("7-bit length marker"),
        };
        if length > MAX_FRAME_BYTES
            || (matches!(opcode, 0x8..=0xA) && length > MAX_CONTROL_PAYLOAD_BYTES)
        {
            return Err(AppError::blocked(
                "CDP WebSocket frame이 허용 크기를 초과했습니다.",
            ));
        }
        let mut payload = vec![0_u8; length];
        self.read_exact(&mut payload)?;
        Ok((opcode, payload))
    }

    fn read_exact(&mut self, output: &mut [u8]) -> Result<(), AppError> {
        let buffered = output.len().min(self.pending.len());
        output[..buffered].copy_from_slice(&self.pending[..buffered]);
        self.pending.drain(..buffered);
        self.stream
            .read_exact(&mut output[buffered..])
            .map_err(|error| {
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                ) {
                    AppError::runtime("CDP WebSocket 응답 제한 시간이 초과했습니다.")
                } else {
                    AppError::runtime("CDP WebSocket frame을 읽지 못했습니다.")
                }
            })
    }
}

fn read_handshake(stream: &mut TcpStream) -> Result<(String, Vec<u8>), AppError> {
    let mut response = Vec::new();
    let mut chunk = [0_u8; 1024];
    loop {
        let count = stream.read(&mut chunk).map_err(|error| {
            if matches!(
                error.kind(),
                std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
            ) {
                AppError::runtime("CDP WebSocket handshake 제한 시간이 초과했습니다.")
            } else {
                AppError::runtime("CDP WebSocket handshake를 읽지 못했습니다.")
            }
        })?;
        if count == 0 {
            return Err(AppError::runtime(
                "CDP WebSocket handshake 중 연결이 종료되었습니다.",
            ));
        }
        response.extend_from_slice(&chunk[..count]);
        if response.len() > MAX_HANDSHAKE_BYTES {
            return Err(AppError::blocked(
                "CDP WebSocket handshake가 허용 크기를 초과했습니다.",
            ));
        }
        if let Some(end) = find_header_end(&response) {
            let headers = String::from_utf8(response[..end].to_vec())
                .map_err(|_| AppError::blocked("CDP WebSocket handshake가 ASCII가 아닙니다."))?;
            return Ok((headers, response[end + 4..].to_vec()));
        }
    }
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|window| window == b"\r\n\r\n")
}

fn validate_handshake(headers: &str, key: &str) -> Result<(), AppError> {
    let mut lines = headers.split("\r\n");
    if lines.next() != Some("HTTP/1.1 101 Switching Protocols") {
        return Err(AppError::blocked(
            "CDP WebSocket endpoint가 protocol upgrade를 거부했습니다.",
        ));
    }
    let mut upgrade = false;
    let mut connection = false;
    let mut accept = None;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            return Err(AppError::blocked(
                "CDP WebSocket handshake header 형식이 올바르지 않습니다.",
            ));
        };
        match name.trim().to_ascii_lowercase().as_str() {
            "upgrade" => upgrade = value.trim().eq_ignore_ascii_case("websocket"),
            "connection" => {
                connection = value
                    .split(',')
                    .any(|token| token.trim().eq_ignore_ascii_case("upgrade"));
            }
            "sec-websocket-accept" => accept = Some(value.trim()),
            _ => {}
        }
    }
    let expected = websocket_accept(key);
    if !upgrade || !connection || accept != Some(expected.as_str()) {
        return Err(AppError::blocked(
            "CDP WebSocket handshake 검증에 실패했습니다.",
        ));
    }
    Ok(())
}

fn websocket_key() -> String {
    let sequence = NONCE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let digest = Sha256::digest(format!("{}:{timestamp}:{sequence}", std::process::id()));
    base64_encode(&digest[..16])
}

fn masking_key() -> [u8; 4] {
    let sequence = NONCE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let digest = Sha256::digest(format!(
        "{}:{sequence}:{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    [digest[0], digest[1], digest[2], digest[3]]
}

fn websocket_accept(key: &str) -> String {
    let mut input = key.as_bytes().to_vec();
    input.extend_from_slice(b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
    base64_encode(&sha1(&input))
}

fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let value = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        output.push(ALPHABET[((value >> 18) & 0x3f) as usize] as char);
        output.push(ALPHABET[((value >> 12) & 0x3f) as usize] as char);
        output.push(if chunk.len() > 1 {
            ALPHABET[((value >> 6) & 0x3f) as usize] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            ALPHABET[(value & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    output
}

fn sha1(input: &[u8]) -> [u8; 20] {
    let bit_len = (input.len() as u64).wrapping_mul(8);
    let mut padded = input.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    let mut state = [
        0x6745_2301_u32,
        0xefcd_ab89,
        0x98ba_dcfe,
        0x1032_5476,
        0xc3d2_e1f0,
    ];
    for chunk in padded.chunks_exact(64) {
        let mut words = [0_u32; 80];
        for (index, bytes) in chunk.chunks_exact(4).enumerate() {
            words[index] = u32::from_be_bytes(bytes.try_into().expect("four bytes"));
        }
        for index in 16..80 {
            words[index] =
                (words[index - 3] ^ words[index - 8] ^ words[index - 14] ^ words[index - 16])
                    .rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = state;
        for (index, word) in words.iter().enumerate() {
            let (f, k) = match index {
                0..=19 => ((b & c) | ((!b) & d), 0x5a82_7999),
                20..=39 => (b ^ c ^ d, 0x6ed9_eba1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8f1b_bcdc),
                _ => (b ^ c ^ d, 0xca62_c1d6),
            };
            let next = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = next;
        }
        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
        state[4] = state[4].wrapping_add(e);
    }
    let mut output = [0_u8; 20];
    for (index, word) in state.iter().enumerate() {
        output[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    output
}

#[cfg(test)]
pub(super) fn test_websocket_accept(key: &str) -> String {
    websocket_accept(key)
}
