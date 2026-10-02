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
    let n = len.max(0) as usize;
    let mut buf = vec![0u8; n];
    if n > 0 {
        let _ = getrandom::getrandom(&mut buf);
    }
    buf
}

pub(crate) fn secure_random_fill(dest: &mut [u8]) {
    if !dest.is_empty() {
        let _ = getrandom::getrandom(dest);
    }
}
