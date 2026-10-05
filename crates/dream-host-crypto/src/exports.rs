// Guest pointers come from the validated runtime C ABI, not arbitrary Rust callers.
#![allow(clippy::missing_safety_doc)]

use dream_host_abi::*;

#[no_mangle]
pub unsafe extern "C" fn cryptoAesGcmEncrypt(
    key: DreamPtr,
    nonce: DreamPtr,
    plaintext: DreamPtr,
    aad: DreamPtr,
) -> DreamPtr {
    use aes_gcm::aead::{Aead, KeyInit, Payload};
    use aes_gcm::{Aes256Gcm, Nonce};
    let key = read_bytes(key);
    let nonce_bytes = read_bytes(nonce);
    let plaintext = read_bytes(plaintext);
    let aad = read_bytes(aad);
    let Ok(cipher) = Aes256Gcm::new_from_slice(&key) else {
        return alloc_bytes(&[]);
    };
    if nonce_bytes.len() != 12 {
        return alloc_bytes(&[]);
    }
    let nonce = Nonce::from_slice(&nonce_bytes);
    match cipher.encrypt(
        nonce,
        Payload {
            msg: &plaintext,
            aad: &aad,
        },
    ) {
        Ok(out) => alloc_bytes(&out),
        Err(_) => alloc_bytes(&[]),
    }
}

#[no_mangle]
pub unsafe extern "C" fn cryptoAesGcmDecrypt(
    key: DreamPtr,
    nonce: DreamPtr,
    ciphertext: DreamPtr,
    aad: DreamPtr,
) -> DreamPtr {
    use aes_gcm::aead::{Aead, KeyInit, Payload};
    use aes_gcm::{Aes256Gcm, Nonce};
    let key = read_bytes(key);
    let nonce_bytes = read_bytes(nonce);
    let ciphertext = read_bytes(ciphertext);
    let aad = read_bytes(aad);
    let Ok(cipher) = Aes256Gcm::new_from_slice(&key) else {
        return alloc_bytes(&[0u8]);
    };
    if nonce_bytes.len() != 12 {
        return alloc_bytes(&[0u8]);
    }
    let nonce = Nonce::from_slice(&nonce_bytes);
    match cipher.decrypt(
        nonce,
        Payload {
            msg: &ciphertext,
            aad: &aad,
        },
    ) {
        Ok(plain) => {
            let mut tagged = Vec::with_capacity(1 + plain.len());
            tagged.push(1u8);
            tagged.extend_from_slice(&plain);
            alloc_bytes(&tagged)
        }
        Err(_) => alloc_bytes(&[0u8]),
    }
}

#[no_mangle]
pub unsafe extern "C" fn cryptoSha256(input: DreamPtr) -> DreamPtr {
    alloc_bytes(&crate::crypto::sha256(&read_bytes(input)))
}

#[no_mangle]
pub unsafe extern "C" fn cryptoSha512(input: DreamPtr) -> DreamPtr {
    alloc_bytes(&crate::crypto::sha512(&read_bytes(input)))
}

#[no_mangle]
pub unsafe extern "C" fn cryptoHmacSha256(key: DreamPtr, input: DreamPtr) -> DreamPtr {
    alloc_bytes(&crate::crypto::hmac_sha256(
        &read_bytes(key),
        &read_bytes(input),
    ))
}

#[no_mangle]
pub extern "C" fn cryptoSecureRandomBytes(len: i32) -> DreamPtr {
    alloc_bytes(&crate::crypto::secure_random(len))
}

#[no_mangle]
pub unsafe extern "C" fn cryptoSecureRandomFill(bytes: DreamPtr) {
    if bytes.is_null() {
        return;
    }
    let n = *(bytes as *const i32);
    if n <= 0 {
        return;
    }
    let dest = std::slice::from_raw_parts_mut(bytes.add(4), n as usize);
    crate::crypto::secure_random_fill(dest);
}
