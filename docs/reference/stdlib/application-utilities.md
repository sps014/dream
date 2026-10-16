# Application utilities

`system.csv`, `system.uuid`, `system.config`, and the temporary-resource helpers in `system.io` cover common application work. Every fallible operation returns `Result`.

## UUID values

`Uuid` is a 16-byte struct containing two unsigned 64-bit words. Assignment copies the value without allocating or retaining a heap object. Equality, ordering, and hashing operate on those bits, so independently parsed UUIDs work as the same map key. Strings and byte arrays are created only when explicitly requested. Parsing accepts canonical hyphenated text in either case and formatting produces lowercase text. `v4()` uses the secure entropy source and reports failures.

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

## Streaming CSV

`CsvReader` borrows a `Reader`; `CsvWriter` borrows a `Writer`. The shared Rust `csv` adapter handles field decoding and quoting on native, Node, and browser targets. The reader frames records incrementally, rejects misplaced/unclosed quotes, validates UTF-8 strictly, and reports a one-based logical record number and absolute byte offset on errors. LF, CRLF, and standalone CR terminate records outside quotes; embedded newlines remain part of quoted fields. Blank records produce one empty field.

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

Records default to a 1 MiB limit and accept a configured limit up to 4 MiB. Reader memory is bounded by that record limit plus its input buffer. A malformed record permanently fails that reader. Writer limits include escaping and the trailing CRLF. Delimiters must be nonzero ASCII excluding quote, CR, and LF. Close the underlying resources explicitly after finishing borrowed wrappers.

## Temporary resources and atomic replacement

`TempDirectory.create()` and `TempFile.create()` securely create a private directory under the OS temporary directory. `create_in(directory)` selects the filesystem. Files live inside that private directory. Explicit `close()` reports cleanup failures and succeeds repeatedly after cleanup; destructors perform best-effort cleanup. Resources are available on native and Node. Browsers report `TempCapabilities.supported == false` and return `EUNSUPPORTED`.

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

Use `TempFile.create_in(Path.parent(destination).unwrap_or("."))` followed by `persist_replace(destination)` to replace a file on the same filesystem. Replacement flushes and closes the temporary file first. A failed rename preserves the old destination and temporary contents. Atomic replacement does not imply durable storage across power loss. An error removing the private directory can occur after a successful replacement; the destination is already published in that case.

Destructors cannot unwind through process termination, traps, or an OS kill. Explicit cleanup is appropriate when reporting cleanup failures matters.

## Typed configuration

`ConfigBuilder` always applies **defaults → JSON files → environment → explicit CLI arguments**, regardless of builder call order. Multiple JSON files apply in insertion order. JSON must be an object of scalar settings and is limited to 1 MiB. Environment names use the prefix plus the ASCII-uppercase key with hyphens replaced by underscores. Environment lookup covers known keys declared by defaults, `require`, validators, files, or CLI definitions. Parser defaults do not override configuration-file or environment values.

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

`get_int`, `get_bool`, `require`, and custom validators return source-aware `ConfigError`s. Diagnostics identify the key and source without including the setting value. Environment variables are unavailable in browser hosts; explicit argument arrays and virtual filesystem JSON files remain usable.

## Async assertions and fixtures

Existing `@test async fun` tests can use `Assert.eq_async(future, expected)` and `Assert.ok_async(future)`. `TestFixture<T, E>` owns a resource and a fallible cleanup callback. `Fixtures.run(fixture, async_body)` runs cleanup after either `Ok` or `Err`, preserves a body error, and otherwise reports cleanup errors. Repeated closure returns the original cleanup outcome. Releasing a cancelled async frame also releases its fixture owner.

`Fixtures.temporary_file()` and `Fixtures.temporary_directory()` return the same owned temporary resource types used by applications. Explicit close reports failures; destructor cleanup is nonthrowing. Assertion failure exits the test process, so use fallible bodies when cleanup must run before a failure is reported.

## Download and transform CSV

Save this as `convert.dream`. It downloads CSV into a temporary file, adds a UUID column
to each record, and replaces the destination only after writing succeeds. The temporary
file keeps memory bounded while allowing the synchronous CSV reader to consume a
network response.

```dream
import system;
import system.cli;
import system.config;
import system.csv;
import system.io;
import system.net;
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
    let received = client.send(HttpRequest.get(parsed_url.unwrap())).await;
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
        .string_option("output", short: "o", default_value: "records.csv");
    let parsed = command.parse(System.args);
    if parsed.is_err() {
        System.stderr.write_line(parsed.unwrap_err().message());
        return 2;
    }
    if parsed.unwrap().is_help {
        System.println(parsed.unwrap().help_text);
        return 0;
    }
    let config = ConfigBuilder().default_value("output", "records.csv").cli(parsed.unwrap()).build();
    if config.is_err() {
        System.stderr.write_line(config.unwrap_err().message());
        return 2;
    }
    let result = convert(
        parsed.unwrap().get_string("url").unwrap(),
        config.unwrap().get_string("output").unwrap()
    ).await;
    if result.is_err() {
        System.stderr.write_line(result.unwrap_err().message());
        return 1;
    }
    System.println("Wrote " + result.unwrap().to_string() + " records");
    return 0;
}
```

Run `dream run convert.dream -- https://example.com/input.csv -o records.csv`, replacing
the URL with an accessible CSV resource. Success prints the number of written records.
Invalid arguments return exit status 2; download, CSV, and filesystem failures return 1.
