import fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { DeclarationGraph } from "./inspect.mjs";
import { emitBindings } from "./emit.mjs";

const directory = path.dirname(fileURLToPath(import.meta.url));
const roots = {
  shared: ["console", "JSON", "URL", "URLSearchParams", "setTimeout", "clearTimeout", "setInterval", "clearInterval", "queueMicrotask"],
};
for (const [name, globals] of Object.entries(roots)) {
  const graph = new DeclarationGraph(directory, "web");
  const module = await graph.load(path.join(directory, "node_modules/@types/web/index.d.ts"), "system.js");
  const shared = await graph.load(path.join(directory, "builtins/shared.d.ts"), "system.js");
  for (const [symbolName, symbol] of shared.symbols) module.symbols.set(symbolName, symbol);
  const output = emitBindings(graph.rootType(module, globals), name, true);
  const namespace = "system.js";
  for (const generated of output.files) generated.source = generated.source.replaceAll(`module dream.js.generated.m${name};`, `module ${namespace};`).replaceAll("module dream.js.types;", "module system.js.bindings;");
  output.files.find(file => file.name === "globals.dream").source = output.files.find(file => file.name === "globals.dream").source.replace("import system.js;", "import system.js;\nimport system.js.bindings;");
  const text = JSON.stringify(output, null, 2) + "\n";
  const destination = path.join(directory, "builtins", `${name}.json`);
  if (process.argv.includes("--check")) {
    if (await fs.readFile(destination, "utf8") !== text) throw new Error(`${name} bindings are stale`);
  } else await fs.writeFile(destination, text);
  console.log(`${name}: ${output.files.length} binding sources`);
}
