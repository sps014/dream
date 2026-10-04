# SecureRandom

**Import:** `import system.crypto;`

Read the [usage guide](../stdlib/crypto.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `static class SecureRandom`

Cryptographically secure random bytes from the OS CSPRNG.

```dream
public static class SecureRandom
```

## `bytes`

Allocates `n` random bytes (`n <= 0` yields an empty array).

```dream
public static fun bytes(n: int): byte[]
```

## `fill`

Fills `dest` in place with random bytes (length unchanged).

```dream
public static fun fill(dest: byte[]): void
```
