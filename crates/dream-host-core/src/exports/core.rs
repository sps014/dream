//! Core guest C ABI: text, cryptography, processes and time zones.

use dream_host_abi::*;

#[no_mangle]
pub unsafe extern "C" fn unicodeNormalize(text: DreamPtr, form: i32) -> DreamPtr {
    use unicode_normalization::UnicodeNormalization;
    let s = read_string(text);
    let out = match form {
        1 => s.nfd().collect::<String>(),
        2 => s.nfkc().collect::<String>(),
        3 => s.nfkd().collect::<String>(),
        _ => s.nfc().collect::<String>(),
    };
    alloc_string(&out)
}

#[no_mangle]
pub unsafe extern "C" fn unicodeToLower(text: DreamPtr) -> DreamPtr {
    alloc_string(&read_string(text).to_lowercase())
}

#[no_mangle]
pub unsafe extern "C" fn unicodeToUpper(text: DreamPtr) -> DreamPtr {
    alloc_string(&read_string(text).to_uppercase())
}

#[no_mangle]
pub unsafe extern "C" fn unicodeGraphemes(text: DreamPtr) -> DreamPtr {
    use unicode_segmentation::UnicodeSegmentation;
    let s = read_string(text);
    let parts: Vec<String> = s.graphemes(true).map(str::to_string).collect();
    alloc_string_array(&parts)
}

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

#[no_mangle]
pub unsafe extern "C" fn processRun(
    command: DreamPtr,
    joined_args: DreamPtr,
    cwd: DreamPtr,
) -> DreamPtr {
    alloc_bytes(&crate::process_host::process_run(
        &read_string(command),
        &read_string(joined_args),
        &read_string(cwd),
    ))
}

#[no_mangle]
pub unsafe extern "C" fn processSpawn(
    command: DreamPtr,
    joined_args: DreamPtr,
    cwd: DreamPtr,
) -> DreamPtr {
    alloc_bytes(&crate::process_host::process_spawn(
        &read_string(command),
        &read_string(joined_args),
        &read_string(cwd),
    ))
}

#[no_mangle]
pub unsafe extern "C" fn processWriteStdin(handle: i32, data: DreamPtr) -> i32 {
    crate::process_host::process_write_stdin(handle, &read_bytes(data))
}

#[no_mangle]
pub extern "C" fn processReadStream(handle: i32, stream: i32, max_bytes: i32) -> DreamPtr {
    alloc_bytes(&crate::process_host::process_read_stream(
        handle, stream, max_bytes,
    ))
}

#[no_mangle]
pub extern "C" fn processReadStreamLine(handle: i32, stream: i32) -> DreamPtr {
    alloc_bytes(&crate::process_host::process_read_stream_line(
        handle, stream,
    ))
}

#[no_mangle]
pub extern "C" fn processWait(handle: i32) -> DreamPtr {
    alloc_bytes(&crate::process_host::process_wait(handle))
}

#[no_mangle]
pub extern "C" fn processKill(handle: i32) -> i32 {
    crate::process_host::process_kill(handle)
}

#[no_mangle]
pub unsafe extern "C" fn dateZoneOffsetMinutes(zone_name: DreamPtr, epoch_millis: i64) -> i32 {
    crate::tz::zone_offset_minutes(&read_string(zone_name), epoch_millis)
}

#[no_mangle]
pub extern "C" fn dateLocalZoneName() -> DreamPtr {
    alloc_string(&crate::tz::local_zone_name())
}
