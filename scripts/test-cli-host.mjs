import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";
import { makeCliHost } from "../runtime/src/hosts/cli.js";
import { cliParserBase64 } from "../runtime/src/hosts/cli_parser.js";

const cases = JSON.parse(fs.readFileSync(new URL("../tests/fixtures/cli/conformance.json", import.meta.url)));
const host = makeCliHost();
function contains(actual, expected) {
  if (expected && typeof expected === "object" && !Array.isArray(expected)) {
    for (const [key, value] of Object.entries(expected)) contains(actual[key], value);
  } else assert.deepEqual(actual, expected);
}
for (const fixture of cases) {
  test(`CLI conformance: ${fixture.name}`, () => contains(JSON.parse(host.cliParse(JSON.stringify(fixture.request))), fixture.expected));
}
test("CLI adapter is self-contained and survives repeated parsing and malformed requests", () => {
  const module = new WebAssembly.Module(Buffer.from(cliParserBase64, "base64"));
  assert.deepEqual(WebAssembly.Module.imports(module), []);
  const request = JSON.stringify(cases[0].request);
  const expected = host.cliParse(request);
  for (let i = 0; i < 100; i++) assert.equal(host.cliParse(request), expected);
  for (const request of ["", "null", "{}", "[1]", '{"command":{}}']) assert.equal(JSON.parse(host.cliParse(request)).kind, "error");
});
