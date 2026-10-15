//! Cross-platform `system.crypto` hosts (sha2 / hmac / getrandom).

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256, Sha512};

type HmacSha256 = Hmac<Sha256>;

pub(crate) fn sha256(data: &[u8]) -> Vec<u8> {
    Sha256::digest(data).to_vec()
}

pub(crate) fn sha512(data: &[u8]) -> Vec<u8> {
    Sha512::digest(data).to_vec()
}

pub(crate) fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC-SHA256 accepts any key length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

pub(crate) fn secure_random(len: i32) -> Vec<u8> {
    secure_random_from(len, getrandom::getrandom)
}

fn secure_random_from(
    len: i32,
    fill: impl FnOnce(&mut [u8]) -> Result<(), getrandom::Error>,
) -> Vec<u8> {
    if !(0..=16 * 1024 * 1024).contains(&len) {
        return Vec::new();
    }
    let mut bytes = vec![0; len as usize];
    if fill(&mut bytes).is_err() {
        return Vec::new();
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn entropy_failure_discards_partial_output() {
        let bytes = secure_random_from(16, |bytes| {
            bytes[0] = 42;
            Err(getrandom::Error::UNSUPPORTED)
        });
        assert!(bytes.is_empty());
        assert_eq!(
            secure_random_from(4, |bytes| {
                bytes.fill(7);
                Ok(())
            }),
            vec![7; 4]
        );
    }
}
