import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { compareReports, measureFiles } from "./check-wasm-size.mjs";

test("deployment measurements count binary companions once and exclude compression sidecars", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "dream-size-"));
  try {
    const main = path.join(root, "main.wasm");
    const js = path.join(root, "main.runtime.js");
    const companion = path.join(root, "codec.wasm");
    fs.writeFileSync(main, Buffer.from([0, 97, 115, 109, 1, 0, 0, 0]));
    fs.writeFileSync(js, "export function load() {}\n");
    fs.writeFileSync(companion, Buffer.alloc(1024, 42));
    const report = measureFiles([main, js, companion, companion]);
    assert.equal(report.artifacts.length, 3);
    assert.equal(report.total.raw, fs.statSync(main).size + fs.statSync(js).size + 1024);
    assert.ok(report.total.brotli < report.total.raw);
    assert.deepEqual(report, measureFiles([companion, js, main]));
    assert.throws(() => measureFiles([main + ".br"]), /original artifacts/);
    const baseline = { schema: 1, fixtures: { codec: report } };
    assert.deepEqual(compareReports(baseline, baseline), []);
    fs.writeFileSync(companion, Buffer.from(Array.from({ length: 1024 }, (_, index) => index % 256)));
    const current = { schema: 1, fixtures: { codec: measureFiles([main, js, companion]) } };
    assert.ok(compareReports(current, baseline).some(message => message.includes("codec: brotli grew")));
    assert.throws(() => compareReports({ schema: 1, fixtures: {} }, baseline), /missing/);
    assert.throws(() => compareReports({ schema: 1, fixtures: { codec: { total: {} } } }, baseline), /invalid/);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});
