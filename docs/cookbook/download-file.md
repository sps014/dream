---
title: Download a file
summary: Stream an HTTP download to disk with a timeout.
icon: material/download-outline
---

# Download a file from the command line

This program accepts a URL, an output path, and a timeout. It rejects an unsuccessful HTTP status, copies the body without loading the whole file, and uses the exit status to report a bad argument or a failed transfer.

Create `download.dream` and add the following three blocks in order.

## 1. Import the APIs

```dream
import system;
import system.cli;
import system.encoding;
import system.io;
import system.http;
```

`system.cli` handles arguments, `system.http` handles HTTP, `system.io` provides the
file and stream APIs, and `system.encoding` turns error messages into bytes for
standard error.

## 2. Download and close resources

The function checks the URL before sending a request, and opens the destination only after a successful HTTP status. Each error path closes the resources already opened.

```dream
async fun download(url: string, path: string, timeout: int): Result<long, string> {
    let parsed = Url.parse(url);
    if parsed.is_err() {
        return Result.Err(parsed.unwrap_err().message());
    }

    let client = HttpClient();
    let sent = client.send(
        HttpRequest(HttpMethod.Get, parsed.unwrap()),
        options: HttpOptions(timeout_ms: timeout, max_redirects: 5)
    ).await;
    if sent.is_err() {
        client.close();
        return Result.Err(sent.unwrap_err().message());
    }
    let response = sent.unwrap();
    if response.status < 200 || response.status >= 300 {
        response.close();
        client.close();
        return Result.Err("HTTP status " + response.status);
    }

    let opened = FileHandle.open_async(path, FileMode.Write).await;
    if opened.is_err() {
        response.close();
        client.close();
        return Result.Err(opened.unwrap_err().message());
    }

    let destination = opened.unwrap();
    let copied = Streams.copy_async(response.body, destination).await;

    let file_closed = destination.close();
    let response_closed = response.close();
    let client_closed = client.close();

    // Preserve the transfer failure while still closing every owned resource.
    if copied.is_err() {
        return Result.Err(copied.unwrap_err().message());
    }
    if file_closed.is_err() {
        return Result.Err(file_closed.unwrap_err().message());
    }
    if response_closed.is_err() {
        return Result.Err(response_closed.unwrap_err().message());
    }
    if client_closed.is_err() {
        return Result.Err(client_closed.unwrap_err().message());
    }

    return Result.Ok(copied.unwrap());
}
```

After copying, it closes the response, the file, and the client, and a transfer error wins over a later cleanup error. A failed transfer can leave a partial file; use a temporary file and `persist_replace` when the previous destination must stay intact until success.

## 3. Parse arguments and report the result

Help and version requests return successfully without starting a download. Argument
errors use exit status 2; transfer failures use 1; success uses 0.

```dream
async fun main(): int {
    let command = Command("download")
        .version("1.0.0")
        .description("Download an HTTP/HTTPS response to a file")
        .positional("url", required: true)
        .string_option("output", letter: "o", default_value: "download.bin")
        .int_option("timeout", default_value: 10000)
        .flag("verbose", letter: "v");

    let result = command.parse(System.args);
    if result.is_err() {
        System.stderr.write_all((result.unwrap_err().message() + "\n").to_utf8()).unwrap();
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

    let timeout = parsed.get_int("timeout").unwrap();
    if timeout < 0 {
        System.stderr.write_all(("timeout must be nonnegative" + "\n").to_utf8()).unwrap();
        System.stderr.flush().unwrap();
        return 2;
    }

    let output = parsed.get_string("output").unwrap();
    let copied = download(parsed.get_string("url").unwrap(), output, timeout).await;
    if copied.is_err() {
        System.stderr.write_all((copied.unwrap_err() + "\n").to_utf8()).unwrap();
        System.stderr.flush().unwrap();
        return 1;
    }
    if parsed.get_bool("verbose").unwrap() {
        System.println("Wrote " + copied.unwrap() + " bytes to " + output);
    }

    return 0;
}
```

## Run it

```sh
dream run download.dream -- --help
dream run download.dream -- https://example.com/file -o download.bin -v
```

Replace the URL with a file you can reach. `-v` prints the byte count and destination after success, the default timeout is 10,000 milliseconds, and the default output is `download.bin`.
