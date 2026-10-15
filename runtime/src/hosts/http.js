import { isNode } from "../platform.js";

const httpClients = new Map();
const httpBodies = new Map();
const httpOperations = new Map();
let nextHttpHandle = 1;
function httpError(code, message) { return { code, message }; }
function httpWireError(error) { return [2, ...new TextEncoder().encode(JSON.stringify(error))]; }
function httpNewOperation(work) {
  if (httpOperations.size >= 1024 || nextHttpHandle >= 2147483647) return 0;
  const id = nextHttpHandle++;
  const op = { result: null, released: false, abort: null, waiters: [] };
  httpOperations.set(id, op);
  Promise.resolve().then(() => work(op)).then(
    result => {
      if (op.released) { result?.dispose?.(); return; }
      op.result = result;
      for (const resolve of op.waiters.splice(0)) resolve(0);
    },
    error => {
      if (!op.released) {
        op.result = { error: error?.code ? error : httpError("ETRANSPORT", "HTTP transport failed") };
        for (const resolve of op.waiters.splice(0)) resolve(0);
      }
    },
  );
  return id;
}
function httpClassify(error, controller, timedOut) {
  if (timedOut()) return httpError("ETIMEOUT", "HTTP request deadline exceeded");
  if (controller.signal.aborted) return httpError("ECLOSED", "HTTP request or body was closed");
  if (error?.code && typeof error.code === "string") return error;
  return httpError("ETRANSPORT", "HTTP transport failed");
}
function httpRestricted(name) {
  return /^(proxy-|sec-)/i.test(name) || /^(accept-charset|accept-encoding|access-control-request-headers|access-control-request-method|connection|content-length|cookie|cookie2|date|dnt|expect|host|keep-alive|origin|permissions-policy|referer|te|trailer|transfer-encoding|upgrade|via|user-agent|x-http-method|x-http-method-override|x-method-override)$/i.test(name);
}
export function makeHttpHost() {
  return {
    netCapabilities: () => isNode ? 6 : 0,
    netClientCreate: () => {
      if (httpClients.size >= 1024 || nextHttpHandle >= 2147483647) return 0;
      const id = nextHttpHandle++;
      httpClients.set(id, { controllers: new Set() });
      return id;
    },
    netClientClose: id => {
      const client = httpClients.get(id);
      httpClients.delete(id);
      for (const controller of client?.controllers || []) controller.abort();
    },
    netHttpStart: (id, request, data) => {
      const bytes = Uint8Array.from(data);
      const client = httpClients.get(id);
      return httpNewOperation(async op => {
        if (!client || !httpClients.has(id)) throw httpError("ECLOSED", "HTTP client is closed");
        let config;
        try { config = JSON.parse(request); } catch (_) { throw httpError("EINVAL", "invalid HTTP request"); }
        const { method, url, headers, timeout_ms, connect_timeout_ms, max_redirects } = config;
        if (!Number.isSafeInteger(timeout_ms) || timeout_ms < 0 || !Number.isSafeInteger(connect_timeout_ms) || connect_timeout_ms < 0 || !Number.isInteger(max_redirects) || max_redirects < -1) throw httpError("EINVAL", "invalid HTTP options");
        if (connect_timeout_ms !== 0) throw httpError("ENOTSUP", "Fetch does not expose connection timeouts");
        if (!isNode && max_redirects !== -1) throw httpError("ENOTSUP", "browser Fetch does not expose redirect limits");
        let target;
        try { target = new URL(url); } catch (_) { throw httpError("EINVAL", "invalid HTTP URL"); }
        if (!["http:", "https:"].includes(target.protocol) || target.username || target.password) throw httpError("EINVAL", "HTTP URL must use http/https without credentials");
        let requestMethod = method;
        let requestBytes = bytes;
        if (!["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"].includes(method)) throw httpError("EINVAL", "unsupported HTTP method");
        if (["GET", "HEAD"].includes(method) && bytes.length) throw httpError("EINVAL", "GET and HEAD bodies are unsupported");
        let requestHeaders = new Headers();
        try {
          for (const [name, value] of headers) {
            if (!isNode && httpRestricted(name)) throw httpError("ENOTSUP", "restricted browser request header");
            requestHeaders.append(name, value);
          }
        } catch (error) { throw error?.code ? error : httpError("EINVAL", "invalid request header"); }
        const controller = new AbortController();
        let timeout = false;
        let timer;
        let disposed = false;
        const dispose = () => {
          if (disposed) return;
          disposed = true;
          clearTimeout(timer);
          client.controllers.delete(controller);
          controller.abort();
        };
        op.abort = dispose;
        if (op.released) { dispose(); throw httpError("ECLOSED", "HTTP operation abandoned"); }
        client.controllers.add(controller);
        if (timeout_ms > 0) timer = setTimeout(() => { timeout = true; controller.abort(); }, timeout_ms);
        try {
          let response;
          let redirects = 0;
          while (true) {
            response = await fetch(target, { method: requestMethod, headers: requestHeaders, body: requestBytes.length ? requestBytes : undefined, redirect: isNode ? "manual" : "follow", signal: controller.signal, credentials: "omit" });
            if (!isNode || ![301, 302, 303, 307, 308].includes(response.status) || max_redirects === 0 || !response.headers.has("location")) break;
            if (++redirects > (max_redirects < 0 ? 10 : max_redirects)) {
              await response.body?.cancel();
              throw httpError("EREDIRECT", "HTTP redirect limit exceeded");
            }
            const next = new URL(response.headers.get("location"), target);
            if (!["http:", "https:"].includes(next.protocol) || next.username || next.password) throw httpError("EINVAL", "invalid redirect URL");
            await response.body?.cancel();
            if (target.origin !== next.origin) {
              for (const name of ["authorization", "proxy-authorization", "cookie", "cookie2"]) requestHeaders.delete(name);
            }
            if (response.status === 303 && requestMethod !== "HEAD" || [301, 302].includes(response.status) && requestMethod === "POST") {
              requestMethod = "GET";
              requestBytes = new Uint8Array();
              for (const name of ["content-type", "content-length", "content-encoding", "content-language", "content-location"]) requestHeaders.delete(name);
            }
            target = next;
          }
          if (response.type === "opaque" || response.type === "opaqueredirect" || response.status === 0) throw httpError("ENOTSUP", "Fetch response metadata is not visible");
          client.controllers.delete(controller);
          const body = { reader: response.body?.getReader(), remainder: new Uint8Array(), offset: 0, busy: false, closed: false, eof: !response.body, controller, dispose, timedOut: () => timeout };
          const responseHeaders = Array.from(response.headers);
          if (typeof response.headers.getSetCookie === "function") {
            for (let i = responseHeaders.length - 1; i >= 0; i--) if (responseHeaders[i][0] === "set-cookie") responseHeaders.splice(i, 1);
            for (const value of response.headers.getSetCookie()) responseHeaders.push(["set-cookie", value]);
          }
          return { response: { status: response.status, url: response.url, headers: responseHeaders }, body, dispose };
        } catch (error) {
          const result = httpClassify(error, controller, () => timeout);
          dispose();
          throw result;
        }
      });
    },
    netBodyRead: (id, count) => {
      const body = httpBodies.get(id);
      // Claim synchronously so two reads submitted before a microtask cannot overlap.
      const busy = body?.busy;
      if (body && !busy) body.busy = true;
      const operationId = httpNewOperation(async op => {
        if (!body || body.closed) throw httpError("ECLOSED", "response body is closed");
        if (busy) throw httpError("EBUSY", "response body is busy");
        op.abort = () => { body.closed = true; body.dispose(); body.busy = false; };
        try {
          if (op.released) { op.abort(); throw httpError("ECLOSED", "HTTP operation abandoned"); }
          if (!Number.isInteger(count) || count < 0) throw httpError("EINVAL", "negative read size");
          if (count === 0) return { bytes: [] };
          while (body.offset === body.remainder.length && !body.eof) {
            const chunk = await body.reader.read();
            body.eof = chunk.done;
            body.remainder = chunk.value || new Uint8Array();
            body.offset = 0;
          }
          const end = Math.min(body.remainder.length, body.offset + count);
          const bytes = Array.from(body.remainder.subarray(body.offset, end));
          body.offset = end;
          if (body.eof) body.dispose();
          return { bytes };
        } catch (error) { throw httpClassify(error, body.controller, body.timedOut); }
        finally { body.busy = false; }
      });
      if (!operationId && body && !busy) body.busy = false;
      return operationId;
    },
    netBodyClose: id => {
      const body = httpBodies.get(id);
      httpBodies.delete(id);
      if (body) {
        body.closed = true;
        body.reader?.cancel().catch(() => {});
        body.dispose();
      }
    },
    netOperationWait: id => {
      const op = httpOperations.get(id);
      if (!op || op.result || op.released) return Promise.resolve(0);
      return new Promise(resolve => op.waiters.push(resolve));
    },
    netOperationPoll: id => {
      const op = httpOperations.get(id);
      if (!op) return httpWireError(httpError("ECLOSED", "HTTP operation is closed"));
      const result = op.result;
      if (!result) return [0];
      op.result = null;
      if (result.error) return httpWireError(result.error);
      if (result.bytes) { op.abort = null; return [1, ...result.bytes]; }
      if (httpBodies.size >= 1024 || nextHttpHandle >= 2147483647) { result.dispose(); return httpWireError(httpError("ELIMIT", "too many response bodies")); }
      const bodyId = nextHttpHandle++;
      httpBodies.set(bodyId, result.body);
      op.responseBody = bodyId;
      op.abort = result.dispose;
      return [1, ...new TextEncoder().encode(JSON.stringify({ ...result.response, body: bodyId }))];
    },
    netOperationClaim: id => {
      const op = httpOperations.get(id);
      if (op) { op.responseBody = null; op.abort = null; }
    },
    netOperationRelease: id => {
      const op = httpOperations.get(id);
      httpOperations.delete(id);
      if (!op) return;
      op.released = true;
      for (const resolve of op.waiters.splice(0)) resolve(0);
      if (op.responseBody) httpBodies.delete(op.responseBody);
      if (op.result?.bytes) { op.abort = null; }
      if (op.result && !op.result.error) op.result.dispose?.();
      if (!op.result) op.abort?.();
    },
  };
}
