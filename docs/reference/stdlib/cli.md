# Command-line arguments

Import `system.cli` to declare commands, named flags, typed options, positional
arguments, and subcommands. Parsing returns `Result<ParsedArguments, CliError>`.
It never prints or exits the process. Help and version requests are successful
outcomes, so your application decides how to display them.

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

Use `command.parse(System.args)` in a native or Node application. `System.args`
is a property containing arguments after the executable/runner, so no executable
name belongs in the array. Browsers support explicit argument arrays through the
same API.

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
Help text and error formatting use the same clap parser on native, Node, and browser.

`CliError.code()` reports `EINVAL` for definitions, `EARGS` for invalid invocations,
`ENOENT` for unknown/missing values, `ETYPE` for incorrect getters, and `EIO` for
adapter failures. Definition limits are 16 nested levels, 256 arguments per
command, 64 children per command, and a 1 MiB serialized request.

## Download application

The complete `sample/cli_download/main.dream` example connects this parser to URL
validation, HTTP streaming, and asynchronous file copying:

```bash
cargo run -- run sample/cli_download/main.dream -- --help
cargo run -- run sample/cli_download/main.dream -- https://example.com/file -o download.bin -v
```

It uses exit status 2 for invalid arguments, 1 for transfer failures, and 0 for
success/help/version. HTTP error statuses are rejected before creating a destination.
Transfers use bounded buffers and explicitly close files, responses, and clients.
A failed transfer can leave a partial destination; atomic replacement is a later
stdlib phase.

[Command declarations](../api/cli-command.md) · [Parsed arguments and errors](../api/cli-parsed-arguments.md)
