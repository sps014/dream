# Built-ins

**Import:** `import system;` for console, env, and process helpers. `Math`, `Buffer`, and primitive methods need no import.

```dream
import system;

fun main() {
    System.println("hello");
    System.println(Math.abs(-3.0));
}
```

## Console

| Call | What it does |
| --- | --- |
| `System.print(x)` | write without a newline |
| `System.println(x)` | write, then newline |
| `System.print_colored(text, ConsoleColor.Yellow)` | one colored string, no newline |
| `System.set_foreground` / `set_background` / `reset_color()` | color until reset |
| `System.clear()` | clear the terminal |
| `System.read_line()` | one line of input |
| `System.read_key()` | one character |
| `System.read_int()` / `read_double()` / `read_bool()` | parse a line → `Result` |
| `System.exit(code)` | stop the process |
| `System.panic(message)` | print and halt — see [Panics](../language/panics.md) |

## Platform and env

| Call | What it does |
| --- | --- |
| `System.platform()` / `System.os_family()` | which host you are on |
| `System.is_browser()` | running in a page? |
| `System.args()` | command-line arguments |
| `System.exe_path()` | path of this executable, if any |
| `System.env(name)` / `env_or(name, fallback)` / `has_env` / `env_keys` | environment variables |
| `System.set_env(name, value)` / `unset_env(name)` | set or clear an env var |
| `System.cwd()` / `set_cwd(path)` | working directory |
| `System.temp_dir()` / `home_dir()` | temp and home paths |

## Math (no import)

`Math.abs`, `floor`, `ceil`, `round`, `sqrt` (returns `Option`), `pow`, `sin` / `cos` / `tan`, `asin` / `acos` / `atan` / `atan2`, `log` / `log10` / `exp` / `hypot`, `min` / `max` / `clamp`, and `Math.PI` / `Math.E`. Angles are radians.

## Timing

`await Time.delay(ms)` / `await Time.sleep(ms)` need an `async` function. `Time.nano_time()` is a monotonic clock. `Stopwatch` times a span — also on [DateTime](datetime.md).

## Other

- `.length` on strings, arrays, and collections.
- `Buffer.alloc<T>(n)` — a zeroed fixed array. Prefer [List](collections.md) when the size grows.
- Override `@override public fun to_string()` so `print` and `+` show your type. See [object](../language/objects.md).
