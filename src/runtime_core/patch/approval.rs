//! Approval credential derivation and fail-closed comparison.

use std::fmt::Write as _;

use sha2::{Digest, Sha256};

pub(crate) const APPROVAL_TOKEN_BYTES: usize = 32;

pub(crate) fn token_from_entropy(entropy: &[u8; APPROVAL_TOKEN_BYTES]) -> String {
    let mut token = String::with_capacity(APPROVAL_TOKEN_BYTES * 2);
    for byte in entropy {
        write!(token, "{byte:02x}").expect("writing to String cannot fail");
    }
    token
}

pub(crate) fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let mut output = String::with_capacity(64);
    for byte in hasher.finalize() {
        write!(output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

pub(crate) fn matches_hash(expected_hash: &str, token: &str) -> bool {
    constant_time_eq(expected_hash, &hash_token(token))
}

fn constant_time_eq(left: &str, right: &str) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.bytes()
        .zip(right.bytes())
        .fold(0_u8, |difference, (a, b)| difference | (a ^ b))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_derivation_and_matching_are_stable() {
        let entropy = [0xabu8; APPROVAL_TOKEN_BYTES];
        let token = token_from_entropy(&entropy);
        let hash = hash_token(&token);

        assert_eq!(token, "ab".repeat(APPROVAL_TOKEN_BYTES));
        assert_eq!(
            hash,
            "271a413bd339c5709fdceaec41f14f11e9fbfb5042d72d331c65f32b284cd09a"
        );
        assert!(matches_hash(&hash, &token));
        assert!(!matches_hash(&hash, "wrong"));
    }
}
