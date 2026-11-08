---
title: CLI arguments
summary: Define commands and parse command-line arguments.
icon: material/console-line
api: reference/api/system-cli.md
---

# Command-line arguments

Import `system.cli` to declare commands, flags, options, positionals, and subcommands. `parse` returns `Result<ParsedArguments, CliError>` and does not print or exit.

Help and version are successful results, so your program decides how to show them. Parsing works the same on every target.

```dream
import system;
import system.cli;

fun main(): void {
    let command = Command("download")
        .version("1.0.0")
        .description("Download a file")
        .positional("url", required: true)
        .string_option("output", letter: "o", default_value: "download.bin")
        .int_option("timeout", default_value: 10000)
        .flag("verbose", letter: "v");

    let parsed = command.parse(["https://example.com/file"]).unwrap();

    System.println(parsed.get_string("output").unwrap());
    System.println(parsed.get_int("timeout").unwrap());
    System.println(parsed.is_present("output"));
}
```

Use `command.parse(System.args)` to parse the program's own arguments. `System.args`
is a property containing arguments after the executable/runner, so no executable
name belongs in the array.

## Command methods

Every builder method returns the same `Command`, so calls chain. Definition
mistakes are reported by `parse`, not by the builder call.

| Method | Parameters (defaults) | Effect |
| --- | --- | --- |
| `Command(name)` | `name: string` | Creates a command named `name`. |
| `version(value)` | `value: string` | Sets version text and enables `--version`/`-V` here and in subcommands. |
| `description(value)` | `value: string` | Adds text to the generated help. |
| `positional(name, ...)` | `required = false`, `multiple = false`, `description = ""` | Adds an ordered string argument. |
| `string_option(name, ...)` | `letter = ""`, `default_value = ""`, `required = false`, `description = ""` | Adds `--name <text>`. |
| `int_option(name, ...)` | `letter = ""`, `default_value = 0`, `required = false`, `description = ""` | Adds `--name <integer>` (signed 32-bit). |
| `flag(name, ...)` | `letter = ""`, `description = ""` | Adds a boolean `--name` that defaults to false. |
| `choices(name, values)` | `values: string[]` | Restricts an already declared positional, string, or integer argument. |
| `subcommand(command)` | `command: Command` | Attaches a snapshot of a child command. |
| `require_subcommand(required)` | `required = true` | Makes invoking one of the subcommands mandatory. |
| `parse(arguments)` | `arguments: string[]` | Returns `Result<ParsedArguments, CliError>`. |

## Options and validation

`string_option` defaults to `""` and `int_option` to `0` unless you set a default. A required option must be passed; its default is ignored.

Flags default to false. Getters check the declared type and return an error for an unknown name or a missing value; an omitted optional positional has no value, and `get_strings` returns an empty array.

Repeated options keep every value. `get_string` and `get_int` return the last one; `get_strings` and `get_ints` return all of them, in order.

Repeating a flag, including `-vv`, keeps it true. `is_present(name)` is true only when the argument was written on the command line.

Integers are signed 32-bit decimals, including `--timeout -42` and `--timeout=-42`. A bad number or overflow returns `EARGS`.

`--` stops option parsing, so a positional may start with `-`. Values keep empty strings, Unicode, spaces, and embedded newlines.

Call `.choices("mode", ["fast", "slow"])` after declaring the argument. A default must be one of those choices.

- A name starts with an ASCII letter, then letters, digits, `_`, or `-`, up to 128 characters. A short option is one ASCII letter.
- A duplicate name or short option, a reserved `help` or `version` name, or a bad positional order returns `EINVAL`.
- Required positionals come before optional ones. Only the last positional can set `multiple: true`.

## Reading parsed values

`ParsedArguments` describes one command scope. Getters return `Result<..., CliError>`
so a misspelled name or a getter of the wrong type is reported instead of guessed.

| Member | Returns | Effect |
| --- | --- | --- |
| `is_help` | `bool` | True when `--help` or `-h` was requested. |
| `help_text` | `string` | Generated help when `is_help` is true; otherwise empty. |
| `is_version` | `bool` | True when `--version` or `-V` was requested. |
| `version_text` | `string` | Version output when `is_version` is true; otherwise empty. |
| `command_name` | `string` | Name of the parsed command; empty for help and version outcomes. |
| `subcommand` | `Option<ParsedArguments>` | The selected child command, or `None`. |
| `is_present(name)` | `bool` | True only when the argument was given on the command line. |
| `get_string(name)` | `Result<string, CliError>` | Last value of a string option or positional. |
| `get_strings(name)` | `Result<string[], CliError>` | All string values in order, including a default. |
| `get_int(name)` | `Result<int, CliError>` | Last value of an integer option. |
| `get_ints(name)` | `Result<int[], CliError>` | All integer values in order. |
| `get_bool(name)` | `Result<bool, CliError>` | Value of a flag; false when omitted. |
| `config_keys()` | `string[]` | Declared argument names, for [configuration merging](application-utilities.md#typed-configuration). |
| `config_value(name)` | `Result<string, CliError>` | Last value of any argument as text. |

## Subcommands and help

```dream
import system;
import system.cli;

fun main(): void {
    let check = Command("check")
        .string_option("mode", default_value: "fast")
        .choices("mode", ["fast", "slow"]);

    let command = Command("tool").version("2.0").subcommand(check).require_subcommand();

    let parsed = command.parse(["check", "--mode", "slow"]).unwrap();
    let child = parsed.subcommand.unwrap();

    System.println(child.command_name);
    System.println(child.get_string("mode").unwrap());
}
```

Finish a subcommand before `subcommand` snapshots it. Each scope has its own values; read the child from `parsed.subcommand` and check `command_name`.

`--help` and `-h` set `is_help` and fill `help_text`, including for a child. After `.version(...)`, `--version` and `-V` set `is_version` and `version_text` on this command and its subcommands, and these requests skip required arguments.

Help shows the description, a usage line, then `Commands:`, `Arguments:`, and `Options:`. Rows add `[default: ...]` and `[possible values: ...]` when set, and an error starts with `error:`, repeats the usage line, and suggests `--help`.

## Error codes

`CliError.code()` returns one of these stable codes; `message()` returns readable
text, and invocation errors include usage guidance.

| Code | Meaning |
| --- | --- |
| `EINVAL` | The command definition is invalid, such as a bad name, duplicate, or misordered positional. |
| `EARGS` | The command line does not match the definition, such as a missing required value or invalid integer. |
| `ENOENT` | A getter named an unknown argument, or the argument has no value. |
| `ETYPE` | The getter does not match the argument's declared type. |

Definition limits are 16 nested levels, 256 arguments per command, and 64 children
per command.

## Handle help and errors

Save this as `inspect.dream`. Handle the parse result before reading arguments, then
check help and version before starting the program's work.

```dream
import system.io;
import system.encoding;
import system;
import system.cli;

fun main(): int {
    let command = Command("inspect").version("1.0.0").positional("path", required: true);

    let result = command.parse(System.args);
    if result.is_err() {
        System.stderr.write_all(Encoding.utf8_encode(result.unwrap_err().message() + "\n")).unwrap();
        System.stderr.flush().unwrap();
        return 2;
    }

    let parsed = result.unwrap();
    if parsed.is_help {
        System.print(parsed.help_text);
        return 0;
    }
    if parsed.is_version {
        System.print(parsed.version_text);
        return 0;
    }

    System.println(parsed.get_string("path").unwrap());
    return 0;
}
```

`dream run inspect.dream -- notes.txt` prints `notes.txt`. `--help` prints usage, `--version` prints `1.0.0`, and a missing path writes to standard error and exits 2.

The [download walkthrough](../../cookbook/download-file.md) applies this pattern to
HTTP transfers, with each step and its cleanup explained on the page.

[Command declarations](../../reference/api/cli-command.md) · [Parsed arguments and errors](../../reference/api/cli-parsed-arguments.md)
