use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::app::AppError;
use sha2::{Digest, Sha256};

pub fn sha256_file(path: &Path) -> Result<String, AppError> {
    let mut file = File::open(path).map_err(|err| {
        AppError::runtime(format!(
            "SHA-256 검증 대상 파일을 열지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];

    loop {
        let bytes_read = file.read(&mut buffer).map_err(|err| {
            AppError::runtime(format!(
                "SHA-256 검증 대상 파일을 읽지 못했습니다: {} ({err})",
                path.display()
            ))
        })?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(bytes_to_hex(&hasher.finalize()))
}

pub fn is_valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn sha256_file_hashes_bytes() {
        let path = std::env::temp_dir().join(format!("rpotato-sha-test-{}", std::process::id()));
        fs::write(&path, b"hello").unwrap();

        let hash = sha256_file(&path).unwrap();

        fs::remove_file(path).unwrap();
        assert_eq!(
            hash,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn sha256_validation_requires_64_hex_chars() {
        assert!(is_valid_sha256(
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        ));
        assert!(!is_valid_sha256("not-a-sha"));
        assert!(!is_valid_sha256(
            "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"
        ));
    }
}
