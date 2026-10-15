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
        .build().unwrap();
    System.println(config.get_int("port").unwrap());
    System.println(config.source("port").unwrap());
}
```

`get_int`, `get_bool`, `require`, and custom validators return source-aware `ConfigError`s. Diagnostics identify the key and source without including the setting value. Environment variables are unavailable in browser hosts; explicit argument arrays and virtual filesystem JSON files remain usable.

## Async assertions and fixtures

Existing `@test async fun` tests can use `Assert.eq_async(future, expected)` and `Assert.ok_async(future)`. `TestFixture<T, E>` owns a resource and a fallible cleanup callback. `Fixtures.run(fixture, async_body)` runs cleanup after either `Ok` or `Err`, preserves a body error, and otherwise reports cleanup errors. Repeated closure returns the original cleanup outcome. Releasing a cancelled async frame also releases its fixture owner.

`Fixtures.temporary_file()` and `Fixtures.temporary_directory()` return the same owned temporary resource types used by applications. Explicit close reports failures; destructor cleanup is nonthrowing. Assertion failure exits the test process, so use fallible bodies when cleanup must run before a failure is reported.

The runnable `sample/http_csv_pipeline` example downloads an HTTP response into a temporary file, reads bounded CSV records, prefixes them with UUIDs, and atomically replaces the output. Downloading to a temporary file permits a synchronous CSV reader to consume an async network body while keeping memory bounded.
