# ConfigBuilder

**Import:** `import system.config;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

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
