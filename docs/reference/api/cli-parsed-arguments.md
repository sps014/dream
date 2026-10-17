# ParsedArguments

**Import:** `import system.cli;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `ParsedArguments`

Immutable parsing outcome for one command scope, including explicit-value provenance and child matches.

```dream
public class ParsedArguments
```

## `is_help`

True when --help or -h was requested, including for a subcommand.

```dream
public get is_help(): bool
```

## `is_version`

True when --version or -V was requested.

```dream
public get is_version(): bool
```

## `help_text`

Deterministic generated help when is_help is true; otherwise empty.

```dream
public get help_text(): string
```

## `version_text`

Generated version text when is_version is true; otherwise empty.

```dream
public get version_text(): string
```

## `command_name`

Command name for successful value outcomes; empty for help/version outcomes.

```dream
public get command_name(): string
```

## `subcommand`

Selected child scope, or None when no subcommand was invoked.

```dream
public get subcommand(): Option<ParsedArguments>
```

## `is_present`

True only for arguments explicitly supplied on the command line, excluding defaults.

```dream
public fun is_present(name: string): bool
```

## `get_string`

Returns a string positional or the last occurrence of a string option; missing values and wrong getter types return errors.

```dream
public fun get_string(name: string): Result<string, CliError>
```

## `get_strings`

Copies all string values in order, including an optional default when no value was supplied.

```dream
public fun get_strings(name: string): Result<string[], CliError>
```

## `get_int`

Returns the last parsed integer; every occurrence has already passed decimal 32-bit validation.

```dream
public fun get_int(name: string): Result<int, CliError>
```

## `get_ints`

Copies every parsed integer occurrence in order.

```dream
public fun get_ints(name: string): Result<int[], CliError>
```

## `get_bool`

Returns the declared boolean flag, false when omitted; wrong getter types return errors.

```dream
public fun get_bool(name: string): Result<bool, CliError>
```

## `config_keys`

Copies declared names for typed configuration merging; is_present distinguishes explicit values.

```dream
public fun config_keys(): string[]
```

## `config_value`

Returns the final explicit scalar value for configuration merging, including typed options.

```dream
public fun config_value(name: string): Result<string, CliError>
```
