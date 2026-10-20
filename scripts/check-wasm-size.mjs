#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { brotliCompressSync, constants, gzipSync } from "node:zlib";

export function measureFiles(files) {
  const unique = [...new Set(files.map(file => path.resolve(file)))].sort();
  const artifacts = unique.map(file => {
    if (/\.(gz|br)$/.test(file)) throw new Error(`measure original artifacts, not compression sidecars: ${file}`);
    const bytes = fs.readFileSync(file);
    return {
      file: path.relative(process.cwd(), file).replaceAll("\\", "/"),
      raw: bytes.length,
      gzip: gzipSync(bytes, { level: 9 }).length,
      brotli: brotliCompressSync(bytes, {
        params: { [constants.BROTLI_PARAM_QUALITY]: 11 },
      }).length,
    };
  });
  const total = { raw: 0, gzip: 0, brotli: 0 };
  for (const artifact of artifacts) {
    for (const encoding of Object.keys(total)) total[encoding] += artifact[encoding];
  }
  return { artifacts, total };
}

export function compareReports(current, baseline) {
  if (baseline.schema !== 1 || current.schema !== 1) throw new Error("unsupported size report schema");
  const regressions = [];
  for (const [name, previous] of Object.entries(baseline.fixtures)) {
    const fixture = current.fixtures[name];
    if (!fixture) throw new Error(`baseline fixture is missing: ${name}`);
    for (const encoding of ["gzip", "brotli"]) {
      if (![fixture.total?.[encoding], previous.total?.[encoding]].every(value => Number.isSafeInteger(value) && value >= 0)) {
        throw new Error(`${name}: invalid ${encoding} total in size report`);
      }
      const delta = fixture.total[encoding] - previous.total[encoding];
      if (delta > 0) regressions.push(`${name}: ${encoding} grew by ${delta} bytes`);
    }
  }
  return regressions;
}

function main(args) {
  const fixtures = new Map();
  let output;
  let baseline;
  for (let index = 0; index < args.length; index++) {
    const option = args[index];
    const value = args[++index];
    if (!value) throw new Error(`missing argument for ${option}`);
    if (option === "--output") output = value;
    else if (option === "--baseline") baseline = value;
    else if (option === "--fixture") {
      const separator = value.indexOf("=");
      if (separator <= 0 || separator === value.length - 1) throw new Error("use --fixture name=artifact-path for each shipped artifact");
      const name = value.slice(0, separator);
      const file = value.slice(separator + 1);
      if (!fixtures.has(name)) fixtures.set(name, []);
      fixtures.get(name).push(file);
    } else throw new Error(`unknown option: ${option}`);
  }
  if (!fixtures.size) throw new Error("supply --fixture name=artifact-path for every main WASM, JS, companion module, and required asset");
  const report = {
    schema: 1,
    fixtures: Object.fromEntries([...fixtures].sort(([left], [right]) => left < right ? -1 : left > right ? 1 : 0)
      .map(([name, files]) => [name, measureFiles(files)])),
  };
  const json = JSON.stringify(report, null, 2) + "\n";
  if (output) {
    fs.mkdirSync(path.dirname(path.resolve(output)), { recursive: true });
    fs.writeFileSync(output, json);
  }
  process.stdout.write(json);
  if (baseline) {
    const regressions = compareReports(report, JSON.parse(fs.readFileSync(baseline, "utf8")));
    if (regressions.length) throw new Error(regressions.join("\n"));
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  try { main(process.argv.slice(2)); }
  catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
