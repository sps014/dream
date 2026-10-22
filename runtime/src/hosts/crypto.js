import { isNode, getNodeCrypto } from "../platform.js";

/** Fills `count` cryptographically secure random bytes (Uint8Array). */
export function csprngBytes(count) {
  const n = count > 0 ? count : 0;
  const out = new Uint8Array(n);
  if (n === 0) return out;
  if (globalThis.crypto && typeof globalThis.crypto.getRandomValues === "function") {
    for (let offset = 0; offset < n; offset += 65536) globalThis.crypto.getRandomValues(out.subarray(offset, Math.min(n, offset + 65536)));
    return out;
  }
  if (isNode && getNodeCrypto()) {
    return Uint8Array.from(getNodeCrypto().randomBytes(n));
  }
  throw new Error("no CSPRNG available");
}

/**
 * AES-256-GCM encrypt/decrypt for the `system.crypto.AesGcm` host ABI. Node uses `node:crypto`
 * (sync `createCipheriv`/`createDecipheriv`); browsers use the async Web Crypto `subtle` API
 * wrapped in a busy-wait on its result since the extern signature is synchronous (matches the
 * native C host, which is also synchronous).
 */
function cryptoAesGcmEncryptBytes(key, nonce, plaintext, aad) {
  const keyBytes = Uint8Array.from(key || []);
  const nonceBytes = Uint8Array.from(nonce || []);
  const ptBytes = Uint8Array.from(plaintext || []);
  const aadBytes = Uint8Array.from(aad || []);
  if (isNode && getNodeCrypto()) {
    const nodeCrypto = getNodeCrypto();
    const cipher = nodeCrypto.createCipheriv("aes-256-gcm", keyBytes, nonceBytes);
    if (aadBytes.length > 0) cipher.setAAD(aadBytes);
    const ct = cipher.update(ptBytes);
    cipher.final();
    const tag = cipher.getAuthTag();
    return Array.from(Buffer.concat([ct, tag]));
  }
  throw new Error("AES-GCM requires node:crypto (browser Web Crypto path is async-only)");
}

function cryptoAesGcmDecryptBytes(key, nonce, ciphertext, aad) {
  const keyBytes = Uint8Array.from(key || []);
  const nonceBytes = Uint8Array.from(nonce || []);
  const ctFull = Uint8Array.from(ciphertext || []);
  const aadBytes = Uint8Array.from(aad || []);
  const tagLen = 16;
  if (ctFull.length < tagLen) return [0];
  const ct = ctFull.subarray(0, ctFull.length - tagLen);
  const tag = ctFull.subarray(ctFull.length - tagLen);
  if (isNode && getNodeCrypto()) {
    try {
      const nodeCrypto = getNodeCrypto();
      const decipher = nodeCrypto.createDecipheriv("aes-256-gcm", keyBytes, nonceBytes);
      decipher.setAuthTag(Buffer.from(tag));
      if (aadBytes.length > 0) decipher.setAAD(aadBytes);
      const pt = Buffer.concat([decipher.update(Buffer.from(ct)), decipher.final()]);
      return [1, ...pt];
    } catch {
      return [0];
    }
  }
  throw new Error("AES-GCM requires node:crypto (browser Web Crypto path is async-only)");
}

export function makeCryptoHost() {
  return {
    cryptoSecureRandomBytes: (n) => {
      if (!Number.isInteger(n) || n < 0 || n > 16777216) return [];
      try { return Array.from(csprngBytes(n)); } catch (_) { return []; }
    },
    cryptoAesGcmEncrypt: (key, nonce, plaintext, aad) =>
      cryptoAesGcmEncryptBytes(key, nonce, plaintext, aad),
    cryptoAesGcmDecrypt: (key, nonce, ciphertext, aad) =>
      cryptoAesGcmDecryptBytes(key, nonce, ciphertext, aad),
  };
}
