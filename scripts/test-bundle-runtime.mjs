import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { bundleRuntime } from "./bundle-runtime.mjs";

test("optimized float square-root libcalls are available in the JS host", () => {
  const source = fs.readFileSync(new URL("../runtime/src/hosts/env.js", import.meta.url), "utf8")
    .replace(/^import .*;$/m, "");
  const make = new Function("getNodeFs", source + "\nreturn defaultEnv;")(() => null);
  const env = make(() => null, {});
  assert.equal(env.sqrtf(4), 2);
  assert.equal(env.sqrtf(2), Math.fround(Math.SQRT2));
  assert.ok(Object.is(env.sqrtf(-0), -0));
  assert.ok(Number.isNaN(env.sqrtf(-1)));
  assert.equal(env.sqrtf(Infinity), Infinity);
});

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

test("runtime bundles are stable across checkout line endings", () => {
  const read = fs.readFileSync;
  try {
    for (const ending of ["\n", "\r\n"]) {
      fs.readFileSync = (file, options) => {
        const value = read(file, options);
        return typeof value === "string" ? value.replace(/\r\n?/g, "\n").replaceAll("\n", ending) : value;
      };
      assert.equal(bundleRuntime(), read(new URL("../runtime/dream.js", import.meta.url), "utf8"));
    }
  } finally {
    fs.readFileSync = read;
  }
});
