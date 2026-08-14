# Random

**Import:** `import system;`

A seedable generator for games, tests, and shuffling — **not** for secrets. For cryptographic randomness use [SecureRandom](crypto.md).

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
