# Command

Import `system.cli`. See the [CLI guide](../../library/system/cli.md) for examples.

## `Command`

Declarative CLI definition. Parsing consumes arguments without an executable name and never exits.

```dream
public class Command
```

## `constructor`

Creates a command; invalid names or conflicting definitions return errors from parse.

```dream
public constructor(name: string)
```

## `version`

Sets version output and enables --version/-V for this command and its descendants.

```dream
public fun version(value: string): Command
```

## `description`

Sets text included in automatically generated help.

```dream
public fun description(value: string): Command
```

## `require_subcommand`

Requires one of the declared subcommands; --help and --version still return normal outcomes.

```dream
public fun require_subcommand(required: bool = true): Command
```

## `positional`

Adds an ordered string positional; only the last positional can collect multiple arguments.

```dream
public fun positional(
    name: string,
    required: bool = false,
    multiple: bool = false,
    description: string = ""
): Command
```

## `string_option`

Adds a string option. Optional values default to empty; repeated occurrences are collected, with the last returned by get_string.

```dream
public fun string_option(
    name: string,
    short: string = "",
    default_value: string = "",
    required: bool = false,
    description: string = ""
): Command
```

## `int_option`

Adds a decimal 32-bit integer option. Optional values default to zero; required options ignore defaults.

```dream
public fun int_option(
    name: string,
    short: string = "",
    default_value: int = 0,
    required: bool = false,
    description: string = ""
): Command
```

## `flag`

Adds a boolean option defaulting to false; repeated occurrences remain true.

```dream
public fun flag(name: string, short: string = "", description: string = ""): Command
```

## `choices`

Restricts a previously declared string, positional, or integer argument to these exact values.

```dream
public fun choices(name: string, values: string[]): Command
```

## `subcommand`

Snapshots a child definition. Later edits to the child do not mutate its attached copy.

```dream
public fun subcommand(command: Command): Command
```

## `parse`

Parses explicit arguments on every target; pass System.args on native/Node. Errors include invalid definitions and conversions.

```dream
public fun parse(arguments: string[]): Result<ParsedArguments, CliError>
```
