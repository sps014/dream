---
title: Random
summary: "Repeatable random numbers for tests, games, and shuffling."
icon: material/dice-multiple-outline
api: reference/api/package-system.md
---

# Random


Use `Random` for tests, games, and shuffling. The same seed repeats the same sequence; use [SecureRandom](../security/crypto.md) when the values must be unpredictable.

**Import:** `import system;`

A seedable generator for games, tests, and shuffling — **not** for secrets. For cryptographic randomness use [SecureRandom](../security/crypto.md).

```dream
import system;

fun main() {
    let rng = Random(42u);
    System.println(rng.next_int(10));
    System.println(rng.next_bool());
}
```

| Call | Meaning |
| --- | --- |
| `Random(seed)` | create with a `uint` seed |
| `next_u32()` | next 32-bit value |
| `next_int(bound)` | `0 .. bound-1` |
| `next_double()` | `0.0 .. 1.0` |
| `next_bool()` | coin flip |
| `next_long()` / `next_bytes(n)` | extra draws |
| `choice(list)` / `shuffle(list)` | pick / Fisher–Yates |
