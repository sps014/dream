# Config

**Import:** `import system.config;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `Config`

Validated snapshot with typed lookups and per-setting provenance.

```dream
public class Config
```

## `source`

Returns the source that supplied the setting, or None if absent.

```dream
public fun source(key: string): Option<string>
```

## `get_string`

Returns a setting or a source-aware missing-setting error.

```dream
public fun get_string(key: string): Result<string, ConfigError>
```

## `get_int`

Parses a signed 32-bit decimal integer, rejecting invalid or overflowing values.

```dream
public fun get_int(key: string): Result<int, ConfigError>
```

## `get_bool`

Accepts the exact boolean spellings true and false.

```dream
public fun get_bool(key: string): Result<bool, ConfigError>
```
