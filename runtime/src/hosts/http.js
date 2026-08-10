/**
 * Performs one HTTP request via the platform `fetch` and serializes the whole response into a single
 * `Uint8Array` for `src/stdlib/net/http_response.dream`: an ASCII head (status line + header lines) and a blank
 * line, then the raw body bytes. Keeping the body raw (an `arrayBuffer`) makes binary responses
 * byte-exact. `body` is either a string or a `Uint8Array` (or "" / empty for none).
 *
 * `httpRequestStream`/`httpRequestStreamBytes` below instead keep the response body's
 * `ReadableStream` reader in a handle table (`streamHandles`) so `httpReadChunk` can pull it
 * incrementally without buffering the whole body — the JS-host mirror of the native host's
 * `reqwest::blocking::Response` handle table (`src/execution/host/http.rs`).
 */
async function httpDo(url, method, headersJson, body, timeoutMs, _httpVersion) {
  const verb = (method || "GET").toUpperCase();
  const init = { method: verb };
  if (headersJson && headersJson !== "") {
    try { init.headers = JSON.parse(headersJson); } catch (_) { /* ignore bad header json */ }
  }
  const hasBody = typeof body === "string" ? body !== "" : body && body.length > 0;
  if (hasBody && verb !== "GET" && verb !== "HEAD") {
    init.body = body;
  }
  const ms = Number(timeoutMs) || 0;
  if (ms > 0) {
    if (typeof AbortSignal !== "undefined" && typeof AbortSignal.timeout === "function") {
      init.signal = AbortSignal.timeout(ms);
    } else {
      const ctrl = new AbortController();
      init.signal = ctrl.signal;
      setTimeout(() => ctrl.abort(), ms);
    }
  }
  try {
    const res = await fetch(url, init);

    let head = `${res.status}\n`;
    res.headers.forEach((value, name) => {
      head += `${name}: ${value}\n`;
    });
    head += "\n"; // blank line separating head from body
    const headBytes = new TextEncoder().encode(head);
    const bodyBytes = new Uint8Array(await res.arrayBuffer());

    const out = new Uint8Array(headBytes.length + bodyBytes.length);
    out.set(headBytes, 0);
    out.set(bodyBytes, headBytes.length);
    return out;
  } catch (e) {
    const msg = (e && (e.name === "TimeoutError" || e.name === "AbortError"))
      ? "timeout"
      : String((e && e.message) || e || "fetch failed");
    const headBytes = new TextEncoder().encode("0\n\n");
    const bodyBytes = new TextEncoder().encode(msg);
    const out = new Uint8Array(headBytes.length + bodyBytes.length);
    out.set(headBytes, 0);
    out.set(bodyBytes, headBytes.length);
    return out;
  }
}

/**
 * Opens a request via `fetch` and returns just the head (status + headers), keeping the response
 * body's `ReadableStream` reader in a handle table for `httpReadChunk` to pull incrementally.
 * Wire format matches [`httpDo`] exactly except the "body" is the decimal handle id, mirroring the
 * native host's `open_http_stream` (`src/execution/host/http.rs`) so `HttpResponse`'s head parser
 * and `HttpStreamResponse` (`wrap_stream` in `http_client.dream`) work unchanged on both hosts.
 */
async function httpDoStream(url, method, headersJson, body, timeoutMs, _httpVersion) {
  const verb = (method || "GET").toUpperCase();
  const init = { method: verb };
  if (headersJson && headersJson !== "") {
    try { init.headers = JSON.parse(headersJson); } catch (_) { /* ignore bad header json */ }
  }
  const hasBody = typeof body === "string" ? body !== "" : body && body.length > 0;
  if (hasBody && verb !== "GET" && verb !== "HEAD") {
    init.body = body;
  }
  const ms = Number(timeoutMs) || 0;
  if (ms > 0) {
    if (typeof AbortSignal !== "undefined" && typeof AbortSignal.timeout === "function") {
      init.signal = AbortSignal.timeout(ms);
    } else {
      const ctrl = new AbortController();
      init.signal = ctrl.signal;
      setTimeout(() => ctrl.abort(), ms);
    }
  }
  try {
    const res = await fetch(url, init);

    let head = `${res.status}\n`;
    res.headers.forEach((value, name) => {
      head += `${name}: ${value}\n`;
    });
    head += "\n";
    const id = nextStreamHandle++;
    streamHandles.set(id, new HttpStreamReader(res.body));
    head += String(id);
    return new TextEncoder().encode(head);
  } catch (e) {
    const msg = (e && (e.name === "TimeoutError" || e.name === "AbortError"))
      ? "timeout"
      : String((e && e.message) || e || "fetch failed");
    const headBytes = new TextEncoder().encode("0\n\n");
    const bodyBytes = new TextEncoder().encode(msg);
    const out = new Uint8Array(headBytes.length + bodyBytes.length);
    out.set(headBytes, 0);
    out.set(bodyBytes, headBytes.length);
    return out;
  }
}

const streamHandles = new Map(); // handle -> HttpStreamReader
let nextStreamHandle = 1;

/** Buffers `ReadableStream` chunks so `httpReadChunk` can hand back exactly `maxBytes` at a time. */
class HttpStreamReader {
  constructor(body) {
    // `fetch` always resolves a body in Node/browsers; a `null` body (e.g. a HEAD response) reads
    // as immediate EOF.
    this.reader = body ? body.getReader() : null;
    this.leftover = null; // Uint8Array not yet handed out
    this.eof = !body;
  }

  async read(maxBytes) {
    const cap = Math.max(1, Number(maxBytes) || 65536);
    if (!this.leftover && !this.eof) {
      const { value, done } = await this.reader.read();
      if (done) {
        this.eof = true;
      } else {
        this.leftover = value;
      }
    }
    if (!this.leftover) {
      return { tag: "eof", bytes: new Uint8Array(0) };
    }
    if (this.leftover.length <= cap) {
      const out = this.leftover;
      this.leftover = null;
      return { tag: "data", bytes: out };
    }
    const out = this.leftover.subarray(0, cap);
    this.leftover = this.leftover.subarray(cap);
    return { tag: "data", bytes: out };
  }

  cancel() {
    if (this.reader) {
      try { this.reader.cancel(); } catch (_) { /* already closed */ }
    }
  }
}

/** Wire: `"data\n<bytes>"` | `"eof\n"` | `"error\n<message>"`, matching `http_read_chunk`. */
async function httpReadChunk(handle, maxBytes) {
  const stream = streamHandles.get(handle);
  if (!stream) {
    return new TextEncoder().encode("error\nstream not found");
  }
  try {
    const { tag, bytes } = await stream.read(maxBytes);
    if (tag === "eof") {
      streamHandles.delete(handle);
    }
    const tagBytes = new TextEncoder().encode(`${tag}\n`);
    const out = new Uint8Array(tagBytes.length + bytes.length);
    out.set(tagBytes, 0);
    out.set(bytes, tagBytes.length);
    return out;
  } catch (e) {
    streamHandles.delete(handle);
    const msg = String((e && e.message) || e || "stream read failed");
    return new TextEncoder().encode(`error\n${msg}`);
  }
}

function httpCloseStream(handle) {
  const stream = streamHandles.get(handle);
  if (!stream) return 0;
  stream.cancel();
  streamHandles.delete(handle);
  return 1;
}

export function makeHttpHost() {
  return {
    // `httpVersion` is accepted for ABI parity with the native host; `fetch` negotiates on its own.
    httpRequest: (url, method, headersJson, body, timeoutMs, httpVersion) =>
      httpDo(url, method, headersJson, body, timeoutMs, httpVersion),
    httpRequestBytes: (url, method, headersJson, body, timeoutMs, httpVersion) =>
      httpDo(url, method, headersJson, Uint8Array.from(body || []), timeoutMs, httpVersion),
    httpRequestStream: (url, method, headersJson, body, timeoutMs, httpVersion) =>
      httpDoStream(url, method, headersJson, body, timeoutMs, httpVersion),
    httpRequestStreamBytes: (url, method, headersJson, body, timeoutMs, httpVersion) =>
      httpDoStream(url, method, headersJson, Uint8Array.from(body || []), timeoutMs, httpVersion),
    httpReadChunk: (handle, maxBytes) => httpReadChunk(handle, maxBytes),
    httpCloseStream: (handle) => httpCloseStream(handle),
  };
}
