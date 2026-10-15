# Config

**Import:** `import system.config;`

## `ConfigError`

Invalid or missing configuration, including the originating source and key.

```dream
public class ConfigError : Error
```

## `public key: string`

Logical setting name associated with the failure.

```dream
public key: string
```

## `public source: string`

The default, JSON filename, environment variable, or CLI argument that supplied it.

```dream
public source: string
```

## `constructor`

Creates a diagnostic with setting provenance.

```dream
public constructor(key: string, source: string, message: string)
```

## `code`

Machine-readable configuration failure code.

```dream
public fun code(): string
```

## `message`

Describes the invalid setting without disclosing its value.

```dream
public fun message(): string
```

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

## `ConfigBuilder`

Merges defaults, JSON files, environment variables, and explicit CLI values in that order.

```dream
public class ConfigBuilder
```

## `constructor`

Creates an empty builder; source precedence is independent of method call order.

```dream
public constructor()
```

## `default_value`

Registers a known setting and its lowest-precedence default.

```dream
public fun default_value(key: string, value: string): ConfigBuilder
```

## `require`

Adds a required known setting with no default.

```dream
public fun require(key: string): ConfigBuilder
```

## `json_file`

Adds a JSON object file; later files override earlier files.

```dream
public fun json_file(path: string): ConfigBuilder
```

## `environment`

Reads known settings from prefix plus the uppercase key, with hyphens replaced by underscores.

```dream
public fun environment(prefix: string = ""): ConfigBuilder
```

## `cli`

Uses explicitly supplied command-line arguments, excluding parser defaults.

```dream
public fun cli(arguments: ParsedArguments): ConfigBuilder
```

## `validate`

Adds a value predicate; false returns a provenance-aware validation error.

```dream
public fun validate(key: string, predicate: fun(string): bool): ConfigBuilder
```

## `build`

Loads and validates a snapshot; failures never change a previously built configuration.

```dream
public fun build(): Result<Config, ConfigError>
```
