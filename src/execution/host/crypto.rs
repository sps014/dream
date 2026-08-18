//! Cryptographic host functions (the `Dream` module behind `system.crypto`). Digests use the `sha2`
//! and `hmac` crates; CSPRNG uses `getrandom`; AES-256-GCM uses the `aes-gcm` crate. Browser/Node
//! hosts mirror these in `runtime/dream.js`.

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256, Sha512};
use wasmtime::*;

use super::memory::{
    read_arg_bytes, with_guest_bytes, with_guest_bytes_mut, write_bytes_to_memory,
};

const LEN_PREFIX: usize = dream_mir::abi::LEN_PREFIX_SIZE as usize;

/// Reads the element count prefix at a `byte[]` data pointer in the caller's linear memory.
fn read_byte_array_len(caller: &mut Caller<'_, ()>, ptr: i32) -> Result<usize> {
    with_guest_bytes(caller, |data| {
        if ptr < 0 {
            return Ok(0);
        }
        let base = ptr as usize;
        let end = base.checked_add(LEN_PREFIX).filter(|&e| e <= data.len());
        let Some(_end) = end else {
            return Ok(0);
        };
        let len = i32::from_le_bytes([data[base], data[base + 1], data[base + 2], data[base + 3]]);
        Ok((len.max(0)) as usize)
    })?
}

/// Overwrites the payload of an existing `byte[]` at `ptr` with `bytes` (truncating to the array length).
fn fill_byte_array_in_memory(caller: &mut Caller<'_, ()>, ptr: i32, bytes: &[u8]) -> Result<()> {
    let count = read_byte_array_len(caller, ptr)?;
    if count == 0 || ptr < 0 {
        return Ok(());
    }
    with_guest_bytes_mut(caller, |data| {
        let base = ptr as usize;
        let start = base + LEN_PREFIX;
        let len = count.min(bytes.len()).min(data.len().saturating_sub(start));
        data[start..start + len].copy_from_slice(&bytes[..len]);
    })
}

/// Registers the `system.crypto` host functions on `linker`.
pub fn link_crypto_functions(linker: &mut Linker<()>) -> Result<()> {
    linker.func_wrap(
        "Dream",
        "cryptoSha256",
        |mut caller: Caller<'_, ()>, data_ptr: i32| -> Result<i32> {
            let data = read_arg_bytes(&mut caller, data_ptr)?;
            let digest = Sha256::digest(&data);
            write_bytes_to_memory(&mut caller, &digest)
        },
    )?;

    linker.func_wrap(
        "Dream",
        "cryptoSha512",
        |mut caller: Caller<'_, ()>, data_ptr: i32| -> Result<i32> {
            let data = read_arg_bytes(&mut caller, data_ptr)?;
            let digest = Sha512::digest(&data);
            write_bytes_to_memory(&mut caller, &digest)
        },
    )?;

    linker.func_wrap(
        "Dream",
        "cryptoHmacSha256",
        |mut caller: Caller<'_, ()>, key_ptr: i32, data_ptr: i32| -> Result<i32> {
            let key = read_arg_bytes(&mut caller, key_ptr)?;
            let data = read_arg_bytes(&mut caller, data_ptr)?;
            let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&key)
                .map_err(|_| Error::msg("invalid HMAC key"))?;
            mac.update(&data);
            write_bytes_to_memory(&mut caller, &mac.finalize().into_bytes())
        },
    )?;

    linker.func_wrap(
        "Dream",
        "cryptoSecureRandomBytes",
        |mut caller: Caller<'_, ()>, n: i32| -> Result<i32> {
            let count = if n > 0 { n as usize } else { 0 };
            let mut out = vec![0u8; count];
            if count > 0 {
                getrandom::getrandom(&mut out)
                    .map_err(|e| Error::msg(format!("getrandom failed: {}", e)))?;
            }
            write_bytes_to_memory(&mut caller, &out)
        },
    )?;

    linker.func_wrap(
        "Dream",
        "cryptoAesGcmEncrypt",
        |mut caller: Caller<'_, ()>,
         key_ptr: i32,
         nonce_ptr: i32,
         plaintext_ptr: i32,
         aad_ptr: i32|
         -> Result<i32> {
            let key = read_arg_bytes(&mut caller, key_ptr)?;
            let nonce_bytes = read_arg_bytes(&mut caller, nonce_ptr)?;
            let plaintext = read_arg_bytes(&mut caller, plaintext_ptr)?;
            let aad = read_arg_bytes(&mut caller, aad_ptr)?;
            let cipher = Aes256Gcm::new_from_slice(&key)
                .map_err(|_| Error::msg("AES-256-GCM key must be 32 bytes"))?;
            let nonce = Nonce::from_slice(&nonce_bytes);
            let ciphertext = cipher
                .encrypt(
                    nonce,
                    Payload {
                        msg: &plaintext,
                        aad: &aad,
                    },
                )
                .map_err(|_| Error::msg("AES-256-GCM encryption failed"))?;
            write_bytes_to_memory(&mut caller, &ciphertext)
        },
    )?;

    linker.func_wrap(
        "Dream",
        "cryptoAesGcmDecrypt",
        |mut caller: Caller<'_, ()>,
         key_ptr: i32,
         nonce_ptr: i32,
         ciphertext_ptr: i32,
         aad_ptr: i32|
         -> Result<i32> {
            let key = read_arg_bytes(&mut caller, key_ptr)?;
            let nonce_bytes = read_arg_bytes(&mut caller, nonce_ptr)?;
            let ciphertext = read_arg_bytes(&mut caller, ciphertext_ptr)?;
            let aad = read_arg_bytes(&mut caller, aad_ptr)?;
            let Ok(cipher) = Aes256Gcm::new_from_slice(&key) else {
                return write_bytes_to_memory(&mut caller, &[0u8]);
            };
            let nonce = Nonce::from_slice(&nonce_bytes);
            match cipher.decrypt(
                nonce,
                Payload {
                    msg: &ciphertext,
                    aad: &aad,
                },
            ) {
                Ok(plaintext) => {
                    // `[1, ...plaintext]`: the leading byte distinguishes "decrypted, and it
                    // happens to be empty" from "authentication failed" without a fallible extern.
                    let mut tagged = Vec::with_capacity(1 + plaintext.len());
                    tagged.push(1u8);
                    tagged.extend_from_slice(&plaintext);
                    write_bytes_to_memory(&mut caller, &tagged)
                }
                Err(_) => write_bytes_to_memory(&mut caller, &[0u8]),
            }
        },
    )?;

    linker.func_wrap(
        "Dream",
        "cryptoSecureRandomFill",
        |mut caller: Caller<'_, ()>, dest_ptr: i32| -> Result<()> {
            let count = read_byte_array_len(&mut caller, dest_ptr)?;
            if count == 0 {
                return Ok(());
            }
            let mut buf = vec![0u8; count];
            getrandom::getrandom(&mut buf)
                .map_err(|e| Error::msg(format!("getrandom failed: {}", e)))?;
            fill_byte_array_in_memory(&mut caller, dest_ptr, &buf)
        },
    )?;

    Ok(())
}
