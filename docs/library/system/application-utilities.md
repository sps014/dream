---
title: Application utilities
summary: "UUIDs, CSV, configuration files, and temporary resources."
icon: material/toolbox-outline
---

# Application utilities

`system.csv`, `system.uuid`, `system.config`, and the temporary-resource helpers in `system.io` cover common application work. Every fallible operation returns `Result`.

## UUID values

`Uuid` is a 16-byte value. Equal bits compare equal, so two parsed copies work as the same map key.

- Parsing accepts canonical hyphenated text in either case. `to_string()` returns lowercase.
- `v4()` uses secure random bytes and reports failures.

```dream
import system;
import system.uuid;

fun main(): void {
    let id = Uuid.parse("550E8400-E29B-41D4-A716-446655440000").unwrap();
    let copy = id;
    System.println(sizeof(Uuid));
    System.println(id == copy);
    System.println(copy.to_string());
}
```

| Member | Returns | Effect |
| --- | --- | --- |
| `Uuid.v4()` | `Result<Uuid, UuidError>` | Generates a random version-4 UUID. |
| `Uuid.parse(text)` | `Result<Uuid, UuidError>` | Parses 36-character hyphenated hexadecimal text. |
| `Uuid.is_valid(text)` | `bool` | Checks the same syntax as `parse` without creating a value. |
| `Uuid.from_bytes(bytes)` | `Result<Uuid, UuidError>` | Copies exactly 16 bytes in network order. |
| `to_bytes()` | `byte[]` | Returns a new 16-byte array in network order. |
| `to_string()` | `string` | Returns canonical lowercase text. |
| `equals(other)` | `bool` | Compares all 128 bits. |
| `compare(other)` | `int` | Orders by the unsigned network-order bytes. |
| `hash_code()` | `int` | Hash for use as a `Map` or `Set` key. |

Failures return `UuidError`, whose `code()` is `EUUID`.

## Streaming CSV

`CsvReader` borrows a `Reader`, and `CsvWriter` borrows a `Writer`. Both work the same on every target.

- Records end with CRLF. A field is quoted when it contains the delimiter, a quote, or a line break, quotes inside are doubled, and a record whose only field is empty is quoted, so `["a", "b,c"]` becomes `a,"b,c"`.
- The reader rejects a misplaced or unclosed quote and invalid UTF-8. The error includes a one-based record number and a byte offset.
- LF, CRLF, and a standalone CR end a record outside quotes. A newline inside quotes stays in the field, and a blank record is one empty field.

```dream
import system;
import system.io;
import system.csv;

fun main(): void {
    let memory = MemoryStream();
    let writer = CsvWriter(memory, delimiter: ';');
    writer.write_record(["Ada;Lovelace", "line\nnext"]).unwrap();
    writer.flush().unwrap();
    memory.seek(0L).unwrap();
    let reader = CsvReader(memory, delimiter: ';');
    let fields = reader.read_record().unwrap().unwrap();
    System.println(fields[0]);
    System.println(fields[1]);
}
```

Records default to a 1 MiB limit, and you can raise that to 4 MiB. Reader memory stays within that limit plus the input buffer.

- A malformed record permanently fails that reader. The writer limit includes escaping and the trailing CRLF.
- The delimiter must be a nonzero ASCII byte other than `"`, CR, or LF.
- Close the underlying stream yourself when you are done with the reader or writer.

| Member | Returns | Effect |
| --- | --- | --- |
| `CsvReader(source, delimiter = ',', max_record_bytes = 1048576)` | `CsvReader` | Reads records from a borrowed `Reader`. |
| `reader.read_record()` | `Result<Option<string[]>, CsvError>` | Returns the next record's fields, or `None` at end of input. |
| `CsvWriter(destination, delimiter = ',', max_record_bytes = 1048576)` | `CsvWriter` | Writes records to a borrowed `Writer`. |
| `writer.write_record(fields)` | `Result<Unit, CsvError>` | Writes one quoted-as-needed record ending in CRLF. |
| `writer.flush()` | `Result<Unit, CsvError>` | Flushes the borrowed destination. |

`CsvError.code()` is `ECSV`. Its `record` field is the one-based record number and
`offset` is the zero-based byte offset where reading or writing failed.

## Temporary resources and atomic replacement

`TempDirectory.create()` and `TempFile.create()` make a private directory under the OS temporary directory. `create_in(directory)` chooses the parent, and the file lives inside that directory.

- `close()` reports cleanup failures and succeeds again after cleanup. Dropping the value also tries to clean up.
- These types work on native and Node. Browsers report `TempCapabilities.supported == false` and return `EUNSUPPORTED`.

```dream
import system;
import system.io;

fun main(): void {
    let temporary = TempFile.create().unwrap();
    temporary.file.write_all(Encoding.utf8_encode("hello")).unwrap();
    temporary.file.seek(0L).unwrap();
    System.println(Encoding.utf8_decode(temporary.file.read(5).unwrap()));
    temporary.close().unwrap();
}
```

Replace a file on the same filesystem with `TempFile.create_in(Path.parent(destination).unwrap_or("."))`, then `persist_replace(destination)`.

- The call flushes and closes the temporary file first. A failed rename leaves the old destination and the temporary file.
- A successful replace is not a promise the data survives power loss. Removing the private directory can still fail after the destination is already replaced.

Call `close()` when you need cleanup errors reported. A crash or a killed process can skip cleanup.

| Member | Returns | Effect |
| --- | --- | --- |
| `TempCapabilities.supported` | `bool` | True on native and Node. |
| `TempDirectory.create()` | `Result<TempDirectory, IoError>` | Creates a private directory under the OS temporary directory. |
| `TempDirectory.create_in(directory)` | `Result<TempDirectory, IoError>` | Creates it inside `directory` instead. |
| `TempDirectory.path` | `string` | Location of the directory; still readable after cleanup. |
| `TempDirectory.close()` | `Result<Unit, IoError>` | Removes the directory and its contents. |
| `TempFile.create()` | `Result<TempFile, IoError>` | Creates a read/write file in a new private directory. |
| `TempFile.create_in(directory)` | `Result<TempFile, IoError>` | Creates the private directory inside `directory`. |
| `TempFile.file` | `FileHandle` | The open file. |
| `TempFile.path` | `string` | Location of the temporary file. |
| `TempFile.persist_replace(destination)` | `Result<Unit, IoError>` | Flushes, closes, and atomically renames the file onto `destination`. |
| `TempFile.close()` | `Result<Unit, IoError>` | Closes the file and removes its private directory. |

Creation returns `EUNSUPPORTED` when temporary resources are unavailable and `ETEMP`
when the operating system refuses to create them.

## Typed configuration

`ConfigBuilder` applies sources in this order, whatever the call order: **defaults, then JSON files, then environment, then explicit CLI arguments**.

- JSON files apply in the order you add them. Each file must be an object of scalar settings and at most 1 MiB.
- An environment name is the prefix plus the key in ASCII uppercase, with hyphens turned into underscores.
- Only keys declared by defaults, `require`, validators, files, or CLI definitions are read from the environment.
- A parser default does not override a file or an environment value.

```dream
import system;
import system.config;
import system.cli;

fun main(): void {
    let command = Command("server").int_option("port", default_value: 80);
    let parsed = command.parse(["--port", "9000"]).unwrap();
    let config = ConfigBuilder()
        .default_value("port", "8080")
        .cli(parsed)
        .validate("port", (value) => int.parse(value).is_ok())
        .build()
        .unwrap();
    System.println(config.get_int("port").unwrap());
    System.println(config.source("port").unwrap());
}
```

`get_int`, `get_bool`, `require`, and validators return `ConfigError` with the key and source, not the value. Browsers have no environment variables; argument arrays and JSON files still work there.

| `ConfigBuilder` method | Effect |
| --- | --- |
| `default_value(key, value)` | Declares `key` with a lowest-precedence default. |
| `require(key)` | Declares `key` and makes `build` fail when no source supplies it. |
| `json_file(path)` | Adds a JSON object file; later files override earlier ones. |
| `environment(prefix = "")` | Enables environment lookup; without this call, no variables are read. |
| `cli(arguments)` | Uses values explicitly given in a `ParsedArguments`, ignoring parser defaults. |
| `validate(key, predicate)` | Fails `build` when `predicate` returns false for the final value. |
| `build()` | Returns `Result<Config, ConfigError>`. |

| `Config` method | Returns | Effect |
| --- | --- | --- |
| `get_string(key)` | `Result<string, ConfigError>` | The final value, or an error when it is missing. |
| `get_int(key)` | `Result<int, ConfigError>` | Parses a signed 32-bit decimal integer. |
| `get_bool(key)` | `Result<bool, ConfigError>` | Accepts exactly `true` or `false`. |
| `source(key)` | `Option<string>` | `"default"`, the JSON file path, the environment variable name, or `"CLI"`. |

`ConfigError.code()` is `ECONFIG`. Its `key` and `source` fields identify the failing
setting, and `message()` combines them with the reason.

## Async assertions and fixtures

`Assert.eq_async` and `Assert.ok_async` await a future inside an `@test async fun`.

`TestFixture<T, E>` owns a resource and a cleanup callback. `Fixtures.run` runs that cleanup after `Ok` or `Err`, keeps a body error, and otherwise reports the cleanup error; calling close again returns the first result.

`Fixtures.temporary_file()` and `Fixtures.temporary_directory()` return the same temporary types applications use. `close()` reports failures, and an assertion failure exits the process, so use a fallible body when cleanup must run first.

| Member | Returns | Effect |
| --- | --- | --- |
| `Assert.eq_async(future, expected)` | `void` (async) | Awaits `future` and asserts it equals `expected`. |
| `Assert.ok_async(future)` | `T` (async) | Awaits a `Result` future, asserts `Ok`, and returns the value. |
| `TestFixture(value, cleanup)` | `TestFixture<T, E>` | Owns `value` and runs `cleanup` once. |
| `fixture.resource` | `T` | The owned value. |
| `fixture.close()` | `Result<Unit, E>` | Runs cleanup, or returns the earlier outcome. |
| `Fixtures.run(fixture, body)` | `Result<Unit, E>` (async) | Runs `body` with the resource, then cleanup. |
| `Fixtures.temporary_file()` | `Result<TempFile, IoError>` | Creates an owned temporary file. |
| `Fixtures.temporary_directory()` | `Result<TempDirectory, IoError>` | Creates an owned private directory. |

## Download and transform CSV

Save this as `convert.dream`. It downloads CSV into a temporary file, adds a UUID column, and replaces the destination only after the write succeeds.

```dream
import system.encoding;
import system;
import system.cli;
import system.config;
import system.csv;
import system.io;
import system.http;
import system.uuid;

fun http_failure(error: HttpError): IoError {
    return IoError.other("", error.message());
}

async fun convert(url: string, output: string): Result<long, IoError> {
    let input = TempFile.create()?;
    let client = HttpClient();
    let parsed_url = Url.parse(url);
    if parsed_url.is_err() {
        return Result.Err(IoError.invalid(url, parsed_url.unwrap_err().message()));
    }
    let received = client.send(HttpRequest(HttpMethod.Get, parsed_url.unwrap())).await;
    if received.is_err() {
        client.close();
        return Result.Err(IoError.other(url, received.unwrap_err().message()));
    }
    let response = received.unwrap();
    if response.status < 200 || response.status >= 300 {
        response.close();
        client.close();
        return Result.Err(IoError.other(url, "HTTP request returned an unsuccessful status"));
    }
    let downloaded = Streams.copy_async(response.body, input.file).await;
    response.close().map_err<IoError>(http_failure)?;
    client.close().map_err<IoError>(http_failure)?;
    downloaded?;
    input.file.seek(0L)?;
    let output_file = TempFile.create_in(Path.parent(output).unwrap_or("."))?;
    let destination = output_file.file;
    let reader = CsvReader(input.file);
    let writer = CsvWriter(destination);
    let count = 0L;
    while true {
        let next = reader.read_record();
        if next.is_err() {
            destination.close();
            return Result.Err(IoError.other(url, next.unwrap_err().message()));
        }
        if next.unwrap().is_none() {
            break;
        }
        let fields = List<string>();
        let id = Uuid.v4();
        if id.is_err() {
            destination.close();
            return Result.Err(IoError.other(output, id.unwrap_err().message()));
        }
        fields.push(id.unwrap().to_string());
        for (let field in next.unwrap().unwrap()) {
            fields.push(field);
        }
        let written = writer.write_record(fields.take_array());
        if written.is_err() {
            destination.close();
            return Result.Err(IoError.other(output, written.unwrap_err().message()));
        }
        count = count + 1L;
    }
    let flushed = writer.flush();
    let closed = destination.close();
    input.close()?;
    if flushed.is_err() {
        return Result.Err(IoError.other(output, flushed.unwrap_err().message()));
    }
    closed?;
    output_file.persist_replace(output)?;
    return Result.Ok(count);
}

async fun main(): int {
    let command = Command("http-csv-pipeline")
        .description("Download CSV and prefix each record with a UUID")
        .positional("url", required: true)
        .string_option("output", letter: "o", default_value: "records.csv");
    let parsed = command.parse(System.args);
    if parsed.is_err() {
        System.stderr.write_all(Encoding.utf8_encode(parsed.unwrap_err().message() + "\n")).unwrap();
        System.stderr.flush().unwrap();
        return 2;
    }
    if parsed.unwrap().is_help {
        System.println(parsed.unwrap().help_text);
        return 0;
    }
    let config = ConfigBuilder().default_value("output", "records.csv").cli(parsed.unwrap()).build();
    if config.is_err() {
        System.stderr.write_all(Encoding.utf8_encode(config.unwrap_err().message() + "\n")).unwrap();
        System.stderr.flush().unwrap();
        return 2;
    }
    let result = convert(
        parsed.unwrap().get_string("url").unwrap(),
        config.unwrap().get_string("output").unwrap()
    ).await;
    if result.is_err() {
        System.stderr.write_all(Encoding.utf8_encode(result.unwrap_err().message() + "\n")).unwrap();
        System.stderr.flush().unwrap();
        return 1;
    }
    System.println("Wrote " + result.unwrap().to_string() + " records");
    return 0;
}
```

Run `dream run convert.dream -- https://example.com/input.csv -o records.csv`. Success prints the record count; invalid arguments exit 2, and download, CSV, or filesystem failures exit 1.
