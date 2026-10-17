# ConfigError

**Import:** `import system.config;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `ConfigError`

Invalid or missing configuration, including the originating source and key.

```dream
public class ConfigError: Error
```

## `key`

Logical setting name associated with the failure.

```dream
public key: string
```

## `source`

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
