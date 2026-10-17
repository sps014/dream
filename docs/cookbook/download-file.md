---
title: Download a file
summary: Stream an HTTP download to disk with a timeout.
icon: material/download-outline
---

# Download a file from the command line

This program accepts a URL, an output path, and a timeout. It rejects unsuccessful
HTTP status codes and copies the response body without loading the whole file into
memory. The exit status tells a script whether argument parsing or the transfer failed.

Create `download.dream` and add the following three blocks in order.

## 1. Import the APIs

```dream
import system;
import system.cli;
import system.io;
import system.net;
```

`system.cli` handles arguments, `system.net` handles HTTP, and `system.io` provides
the file and stream APIs.

## 2. Download and close resources

The function validates the URL before sending a request. It opens the destination
only after receiving a successful HTTP status. Each error path closes the resources
that have already been opened.

```dream
async fun download(url: string, path: string, timeout: int): Result<long, string> {
    let parsed = Url.parse(url);
    if parsed.is_err() {
        return Result.Err(parsed.unwrap_err().message());
    }

    let client = HttpClient();
    let sent = client.send(
        HttpRequest.get(parsed.unwrap()),
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

    let opened = FileHandle.open_async(path, "w").await;
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

After copying, it attempts all three close operations. A transfer error takes
precedence over a later cleanup error. A failed transfer can leave a partial file;
use a temporary file followed by `persist_replace` when the previous destination
must remain intact until success.

## 3. Parse arguments and report the result

Help and version requests return successfully without starting a download. Argument
errors use exit status 2; transfer failures use 1; success uses 0.

```dream
import system.io;
import system.encoding;

async fun main(): int {
    let command = Command("download")
        .version("1.0.0")
        .description("Download an HTTP/HTTPS response to a file")
        .positional("url", required: true)
        .string_option("output", short: "o", default_value: "download.bin")
        .int_option("timeout", default_value: 10000)
        .flag("verbose", short: "v");

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

    let timeout = parsed.get_int("timeout").unwrap();
    if timeout < 0 {
        System.stderr.write_all(Encoding.utf8_encode("timeout must be nonnegative" + "\n")).unwrap();
        System.stderr.flush().unwrap();
        return 2;
    }

    let output = parsed.get_string("output").unwrap();
    let copied = download(parsed.get_string("url").unwrap(), output, timeout).await;
    if copied.is_err() {
        System.stderr.write_all(Encoding.utf8_encode(copied.unwrap_err() + "\n")).unwrap();
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

Replace the URL with an accessible resource. `-v` prints the byte count and destination
after success. The default timeout is 10,000 milliseconds and the default output is
`download.bin`.
