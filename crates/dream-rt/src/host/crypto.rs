use crate::guest;
use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256, Sha512};

#[no_mangle]
pub extern "C" fn dream_crypto_sha256(data: i32) -> i32 {
    guest::write_bytes(&Sha256::digest(guest::read_bytes(data)))
}

#[no_mangle]
pub extern "C" fn dream_crypto_sha512(data: i32) -> i32 {
    guest::write_bytes(&Sha512::digest(guest::read_bytes(data)))
}

#[no_mangle]
pub extern "C" fn dream_crypto_hmac_sha256(key: i32, data: i32) -> i32 {
    let key = guest::read_bytes(key);
    let data = guest::read_bytes(data);
    let Ok(mut mac) = <Hmac<Sha256> as Mac>::new_from_slice(&key) else {
        return guest::write_bytes(&[]);
    };
    mac.update(&data);
    guest::write_bytes(&mac.finalize().into_bytes())
}

#[no_mangle]
pub extern "C" fn dream_crypto_secure_random_bytes(n: i32) -> i32 {
    let count = n.max(0) as usize;
    let mut out = vec![0u8; count];
    if count > 0 {
        let _ = getrandom::getrandom(&mut out);
    }
    guest::write_bytes(&out)
}

#[no_mangle]
pub extern "C" fn dream_crypto_secure_random_fill(dest: i32) {
    let n = guest::read_bytes(dest).len();
    let mut buf = vec![0u8; n];
    if n > 0 {
        let _ = getrandom::getrandom(&mut buf);
    }
    guest::fill_bytes(dest, &buf);
}

#[no_mangle]
pub extern "C" fn dream_crypto_aes_gcm_encrypt(key: i32, nonce: i32, plain: i32, aad: i32) -> i32 {
    let key = guest::read_bytes(key);
    let nonce_b = guest::read_bytes(nonce);
    let plaintext = guest::read_bytes(plain);
    let aad = guest::read_bytes(aad);
    let Ok(cipher) = Aes256Gcm::new_from_slice(&key) else {
        return guest::write_bytes(&[]);
    };
    if nonce_b.len() != 12 {
        return guest::write_bytes(&[]);
    }
    let nonce = Nonce::from_slice(&nonce_b);
    match cipher.encrypt(
        nonce,
        Payload {
            msg: &plaintext,
            aad: &aad,
        },
    ) {
        Ok(c) => guest::write_bytes(&c),
        Err(_) => guest::write_bytes(&[]),
    }
}

#[no_mangle]
pub extern "C" fn dream_crypto_aes_gcm_decrypt(key: i32, nonce: i32, cipher_ptr: i32, aad: i32) -> i32 {
    let key = guest::read_bytes(key);
    let nonce_b = guest::read_bytes(nonce);
    let ciphertext = guest::read_bytes(cipher_ptr);
    let aad = guest::read_bytes(aad);
    let Ok(cipher) = Aes256Gcm::new_from_slice(&key) else {
        return guest::write_bytes(&[0u8]);
    };
    if nonce_b.len() != 12 {
        return guest::write_bytes(&[0u8]);
    }
    let nonce = Nonce::from_slice(&nonce_b);
    match cipher.decrypt(
        nonce,
        Payload {
            msg: &ciphertext,
            aad: &aad,
        },
    ) {
        Ok(p) => {
            let mut tagged = Vec::with_capacity(1 + p.len());
            tagged.push(1);
            tagged.extend_from_slice(&p);
            guest::write_bytes(&tagged)
        }
        Err(_) => guest::write_bytes(&[0u8]),
    }
}
