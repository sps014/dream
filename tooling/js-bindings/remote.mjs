import fs from "node:fs/promises";
import path from "node:path";
import { createHash } from "node:crypto";
import { parseSync, visitorKeys } from "oxc-parser";

const hash = value => createHash("sha256").update(value).digest("hex");

export async function remote(url, root, seen = new Map()) {
  const address = new URL(url);
  if (address.protocol !== "https:" || address.username || address.password) throw new Error(`remote JS imports require an HTTPS URL without credentials: ${url}`);
  if (seen.has(url)) return seen.get(url);
  const cache = path.join(root, ".dream", "js", "remote");
  await fs.mkdir(cache, { recursive: true });
  const lockfile = path.join(root, "dream.js.lock");
  const lock = JSON.parse(await fs.readFile(lockfile, "utf8").catch(() => '{"version":1,"modules":{}}'));
  if (lock.version !== 1 || !lock.modules) throw new Error("unsupported dream.js.lock format");
  const key = hash(url).slice(0, 32);
  const source = path.join(cache, `${key}.source`);
  const file = path.join(cache, `${key}${/\.d\.[cm]?ts(?:\?|$)/.test(url) ? ".d.mts" : ".mjs"}`);
  let code = await fs.readFile(source, "utf8").catch(() => null);
  let pin = lock.modules[url];
  if (code === null) {
    if (process.env.DREAM_NO_AUTO_INSTALL === "1") throw new Error(`remote module is not cached: ${url}`);
    const response = await fetch(url, { signal: AbortSignal.timeout(30_000) });
    if (!response.ok) throw new Error(`downloading ${url}: HTTP ${response.status}`);
    if (!response.url.startsWith("https://")) throw new Error(`remote module redirects outside HTTPS: ${url}`);
    code = await response.text();
    if (Buffer.byteLength(code) > 20 * 1024 * 1024) throw new Error(`remote module exceeds 20 MiB: ${url}`);
    const integrity = hash(code);
    if (pin && pin.integrity !== integrity) throw new Error(`remote module content changed since it was locked: ${url}`);
    const types = response.headers.get("x-typescript-types");
    pin = { integrity, url: response.url, ...(types ? { types: new URL(types, response.url).href } : {}) };
    await fs.writeFile(source, code);
  }
  if (pin && hash(code) !== pin.integrity) throw new Error(`cached remote module integrity mismatch: ${url}`);
  pin ??= { integrity: hash(code), url };
  lock.modules[url] = pin;
  await fs.writeFile(lockfile, JSON.stringify({ version: 1, modules: Object.fromEntries(Object.entries(lock.modules).sort()) }, null, 2) + "\n");
  seen.set(url, file);
  const parsed = parseSync(file, code, { astType: "ts" });
  if (parsed.errors.length) throw new Error(`${url}: ${parsed.errors[0].message}`);
  const ast = parsed.program;
  const edits = [];
  const specs = [];
  function visit(node) {
    if (!node || typeof node !== "object") return;
    if (["ImportDeclaration", "ExportNamedDeclaration", "ExportAllDeclaration", "ImportExpression"].includes(node.type) && node.source) specs.push(node.source);
    for (const key of visitorKeys[node.type] ?? []) {
      const child = node[key];
      if (Array.isArray(child)) child.forEach(visit); else visit(child);
    }
  }
  visit(ast);
  for (const spec of specs) {
    if (spec.type !== "Literal" || typeof spec.value !== "string") throw new Error(`remote module has a non-literal dynamic import: ${url}`);
    const value = spec.value;
    if (!value.startsWith(".") && !value.startsWith("/") && !value.startsWith("https://")) throw new Error(`remote module ${url} contains an unresolved bare import '${value}'`);
    const resolved = await remote(new URL(value, pin.url).href, root, seen);
    edits.push([spec.start, spec.end, JSON.stringify(`./${path.basename(resolved)}`)]);
  }
  for (const [start, end, replacement] of edits.sort((a, b) => b[0] - a[0])) code = code.slice(0, start) + replacement + code.slice(end);
  await fs.writeFile(file, code);
  if (pin.types && !file.endsWith(".d.mts")) {
    const declarations = await remote(pin.types, root, seen);
    await fs.copyFile(declarations, file.replace(/\.mjs$/, ".d.mts"));
  }
  return file;
}
