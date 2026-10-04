# System

**Import:** `import system;`

Read the [usage guide](../stdlib/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](system.md)

## `read_line`

Reads a line of input from stdin.

```dream
public static fun read_line(): string
```

## `read_key`

Reads a single keypress from stdin.

```dream
public static fun read_key(): char
```

## `read_int`

Reads a line from stdin and parses it as an integer.

```dream
public static fun read_int(): Result<int, ParseError>
```

## `read_double`

Reads a line from stdin and parses it as a double.

```dream
public static fun read_double(): Result<double, ParseError>
```

## `read_bool`

Reads a line from stdin and parses it as a bool (`true`/`false`).

```dream
public static fun read_bool(): Result<bool, ParseError>
```

## `exit`

Terminates the process with the given exit code.

```dream
public static fun exit(code: int): void
```

## `platform`

The current host platform.

```dream
public static get platform(): Platform
```

## `os_family`

The OS family on native hosts (Unknown elsewhere).

```dream
public static get os_family(): OsFamily
```

## `cpu_time_nanos`

User+system CPU time of this process since it started, in nanoseconds (`0` if unknown).

```dream
public static get cpu_time_nanos(): long
```

## `memory_bytes`

Resident process memory in bytes (`0` if unknown). Not `Debug.live_objects`.

```dream
public static get memory_bytes(): long
```

## `is_browser`

True when running under a browser JS host.

```dream
public static get is_browser(): bool
```

## `args`

Process arguments (excluding the executable name when the host provides one separately).

```dream
public static get args(): string[]
```

## `exe_path`

Absolute path of the running executable when known.

```dream
public static get exe_path(): Option<string>
```

## `env`

Value of environment variable `name`, or `None` when unset.

```dream
public static fun env(borrow name: string): Option<string>
```

## `env_or`

Value of environment variable `name`, or `fallback` when unset.

```dream
public static fun env_or(borrow name: string, fallback: string): string
```

## `set_env`

Sets environment variable `name` to `value`.

```dream
public static fun set_env(borrow name: string, borrow value: string): Result<bool, ArgError>
```

## `has_env`

True when environment variable `name` is set.

```dream
public static fun has_env(borrow name: string): bool
```

## `unset_env`

Unsets environment variable `name`.

```dream
public static fun unset_env(borrow name: string): Result<bool, ArgError>
```

## `env_keys`

Names of all environment variables (empty on hosts without an env block).

```dream
public static get env_keys(): string[]
```

## `temp_dir`

Process temp directory (`TMPDIR` / `GetTempPath` / `"/tmp"` in the browser).

```dream
public static get temp_dir(): string
```

## `home_dir`

User home directory when the host provides one.

```dream
public static get home_dir(): Option<string>
```

## `cwd`

Current working directory.

```dream
public static get cwd(): Result<string, IoError>
```

## `set_cwd`

Sets the current working directory.

```dream
public static fun set_cwd(borrow path: string): Result<bool, IoError>
```

## `clear`

Clears the terminal screen.

```dream
public static fun clear(): void
```
