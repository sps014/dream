---
title: API errors
summary: Understand and handle failures reported by library calls.
icon: material/alert-circle-outline
---

# Understand an API failure

An operation that can fail usually returns `Result<Value, ErrorType>`. Check `Ok` and `Err` rather than assuming it worked. The error's `message()` is for people; its `code()` lets your program distinguish categories of failure.

```dream
import system;

fun main() {
    switch (int.parse("not a number")) {
        Ok(value) => System.println(value),
        Err(error) => System.println(error.code()),
    }
}
```

The parse operation takes the error branch. Read [Option and Result](option-result.md) for defaults, recovery, and forwarding a failure with `?`.

## Choose the error guide

| Operation | Error type | Reference |
| --- | --- | --- |
| Parse a value | `ParseError` | [ParseError](../../reference/api/core-parse-error.md) |
| Read or write a file | `IoError` | [IoError](../../reference/api/io-io-error.md) |
| Run another process | `ProcessError` | [ProcessError](../../reference/api/process-process-error.md) |
| Encrypt or decrypt | `CryptoError` | [CryptoError](../../reference/api/crypto-crypto-error.md) |

## Encryption errors

`CryptoError` is returned by encryption and key operations. `EINVAL_KEY` indicates unsuitable key material, `EINVAL_NONCE` an invalid nonce, and `EDECRYPT` a decryption or authentication failure. Handle `EDECRYPT` as a failed operation; do not use unauthenticated output.

The public helpers `invalid_key`, `invalid_nonce`, and `decrypt_failed` create errors from a message. `CryptoError(code, message)` constructs one directly.

## Cancellation and fatal errors

Async APIs with a cancellation token may report `ECANCELLED`; check the specific method's contract. A [panic](../../language/rules/panics.md) stops the program and is not a returned error you can recover from.
