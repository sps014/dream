# System

**Import:** `import system;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `print`

Writes and flushes a value as console text, as a shortcut for `System.stdout`.

```dream
public static extern fun print<T>(borrow value: T): void
```

## `println`

Writes and flushes a value followed by a newline, as a shortcut for `System.stdout`.

```dream
public static extern fun println<T>(borrow value: T): void
```

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

## `set_env`

Sets environment variable `name` to `value`.

```dream
public static fun set_env(borrow name: string, borrow value: string): Result<Unit, ArgumentError>
```

## `unset_env`

Unsets environment variable `name`.

```dream
public static fun unset_env(borrow name: string): Result<Unit, ArgumentError>
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
public static fun set_cwd(borrow path: string): Result<Unit, IoError>
```

## `clear`

Clears the terminal screen.

```dream
public static fun clear(): void
```

## `set_foreground`

Sets the foreground color for subsequent output.

```dream
public static fun set_foreground(color: ConsoleColor): void
```

## `set_background`

Sets the background color for subsequent output.

```dream
public static fun set_background(color: ConsoleColor): void
```

## `reset_color`

Resets console colors to the terminal default.

```dream
public static fun reset_color(): void
```
