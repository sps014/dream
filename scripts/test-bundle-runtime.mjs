import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { bundleRuntime } from "./bundle-runtime.mjs";

test("runtime bundle is independent of platform path separators", () => {
  const expected = fs.readFileSync(new URL("../runtime/dream.js", import.meta.url), "utf8");
  assert.equal(bundleRuntime(), expected);
  const relative = path.relative;
  const sep = path.sep;
  try {
    path.relative = (...args) => relative(...args).replaceAll("/", "\\");
    path.sep = "\\";
    assert.equal(bundleRuntime(), expected);
  } finally {
    path.relative = relative;
    path.sep = sep;
  }
});
