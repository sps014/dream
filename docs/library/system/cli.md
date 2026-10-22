---
title: CLI arguments
summary: Define commands and parse command-line arguments.
icon: material/console-line
api: reference/api/system-cli.md
---

# Command-line arguments

Import `system.cli` to declare commands, named flags, typed options, positional
arguments, and subcommands. Parsing returns `Result<ParsedArguments, CliError>`.
It never prints or exits the process. Help and version requests are successful
outcomes, so your application decides how to display them.
Parsing behaves the same on native, Node, browser, and standalone WASI targets.

```dream
import system;
import system.cli;

fun main(): void {
    let command = Command("download")
        .version("1.0.0")
        .description("Download a file")
        .positional("url", required: true)
        .string_option("output", short: "o", default_value: "download.bin")
        .int_option("timeout", default_value: 10000)
        .flag("verbose", short: "v");

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
| `string_option(name, ...)` | `short = ""`, `default_value = ""`, `required = false`, `description = ""` | Adds `--name <text>`. |
| `int_option(name, ...)` | `short = ""`, `default_value = 0`, `required = false`, `description = ""` | Adds `--name <integer>` (signed 32-bit). |
| `flag(name, ...)` | `short = ""`, `description = ""` | Adds a boolean `--name` that defaults to false. |
| `choices(name, values)` | `values: string[]` | Restricts an already declared positional, string, or integer argument. |
| `subcommand(command)` | `command: Command` | Attaches a snapshot of a child command. |
| `require_subcommand(required)` | `required = true` | Makes invoking one of the subcommands mandatory. |
| `parse(arguments)` | `arguments: string[]` | Returns `Result<ParsedArguments, CliError>`. |

## Options and validation

`string_option` defaults to an empty string and `int_option` to zero unless you
supply a default. A required option needs an explicit value; its default is ignored.
Flags default to false. `get_string`, `get_int`, and `get_bool` enforce the declared
type and return errors for unknown names or missing values. An omitted optional
positional has no value; `get_strings` returns an empty array for it.

Repeated options retain all occurrences. `get_string` and `get_int` return the last
one; `get_strings` and `get_ints` copy all occurrences in order. Repeating a boolean
flag keeps it true, including short clusters such as `-vv`. `is_present(name)`
distinguishes explicit arguments from defaults.

Integers are signed decimal 32-bit values, including negative values supplied as
`--timeout -42` or `--timeout=-42`. Invalid conversions and overflow return `EARGS`.
`--` ends option parsing, allowing positionals beginning with `-`. Argument values
preserve empty strings, Unicode, whitespace, and embedded newlines.

Use `.choices("mode", ["fast", "slow"])` after declaring an argument to restrict
its values. An optional default must also be one of those choices. Command and
argument names start with an ASCII letter and contain ASCII letters, digits, `_`,
or `-`, with a maximum of 128 characters. Short options are one ASCII letter. Duplicate names/short options,
reserved `help`/`version` names, and invalid positional ordering return `EINVAL`.
Required positionals precede optional ones; only the last can have `multiple: true`.

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

An attached subcommand is a snapshot: finish configuring it before attaching it.
Each parsed command scope has its own values. A selected child is available through
`parsed.subcommand`; use `command_name` to identify it.

`--help`/`-h` return `is_help` with generated `help_text`, including for children.
After `.version(...)`, `--version`/`-V` return `is_version` with `version_text`;
versions propagate to subcommands. These requests bypass required argument checks.
Help lists the description, a usage line, then `Commands:`, `Arguments:`, and`
Options:` sections. Each row shows the argument's description followed by any`
[default: ...]` and `[possible values: ...]`, aligned two spaces after the widest
entry in its section. Invocation errors start with `error:`, then repeat the usage line
and suggest `--help`.

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

Run `dream run inspect.dream -- notes.txt` to print `notes.txt`.
Use `--help` to print usage, or `--version` to print `1.0.0`. Missing the required
path prints an error to standard error and returns exit status 2.

The [download walkthrough](../../cookbook/download-file.md) applies this pattern to
HTTP transfers, with each step and its cleanup explained on the page.

[Command declarations](../../reference/api/cli-command.md) · [Parsed arguments and errors](../../reference/api/cli-parsed-arguments.md)
