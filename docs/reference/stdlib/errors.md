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
| Parse a value | `ParseError` | [ParseError](../api/core-parse-error.md) |
| Read or write a file | `IoError` | [IoError](../api/io-io-error.md) |
| Make an HTTP request | `HttpError` | [HttpError](../api/net-http-error.md) |
| Use a socket | `NetError` | [NetError](../api/net-net-error.md) |
| Run another process | `ProcessError` | [ProcessError](../api/process-process-error.md) |
| Use a GPU | `GpuError` | [GpuError](../api/gpu-gpu-error.md) |
| Encrypt or decrypt | `CryptoError` | [CryptoError](../api/crypto-crypto-error.md) |
| Open or use a web window | `WebViewError` | [WebViewError](../api/webview-webview-error.md) |
| Use a desktop dialog or clipboard | `DesktopError` | [DesktopError](../api/desktop-desktop-error.md) |

## Encryption errors

`CryptoError` is returned by encryption and key operations. `EINVAL_KEY` indicates unsuitable key material, `EINVAL_NONCE` an invalid nonce, and `EDECRYPT` a decryption or authentication failure. Handle `EDECRYPT` as a failed operation; do not use unauthenticated output.

The public helpers `invalid_key`, `invalid_nonce`, and `decrypt_failed` create errors from a message. `CryptoError(code, message)` constructs one directly.

## Window errors

`WebViewError` uses `UNAVAILABLE`, `EFAILED`, `EUNSUPPORTED`, `EPARSE`, and `ECANCELLED` for unavailable services, operation failures, unsupported operations, invalid responses, and cancellation. Its helpers are `unavailable(message)`, `failed(message)`, `unsupported(message)`, `parse(message)`, and `cancelled()`.

## Cancellation and fatal errors

Async APIs with a cancellation token may report `ECANCELLED`; check the specific method's contract. A [panic](../language/panics.md) stops the program and is not a returned error you can recover from.
