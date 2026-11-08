import path from "node:path";
import { fileURLToPath } from "node:url";
import * as esbuild from "esbuild";
import { dependencies } from "./dependencies.mjs";
import { digest, resolveImport } from "./resolve.mjs";
import { DeclarationGraph } from "./inspect.mjs";
import { emitBindings } from "./emit.mjs";

export async function processImports(request) {
  const directory = await dependencies(request.root, request.npm);
  if (request.mode === "bundle") return bundleModules(request, directory);
  const modules = [];
  const graph = new DeclarationGraph(request.root, request.target);
  for (const entry of request.imports) {
    const resolved = await resolveImport(entry, request, directory);
    const id = digest(`${entry.specifier}\n${path.relative(request.root, entry.file)}`);
    const containing = resolved.startsWith(".") || path.isAbsolute(resolved) ? path.dirname(resolved) : directory;
    const file = graph.resolve(path.join(containing, "__dream_import__.mjs"), resolved);
    const module = await graph.load(file, entry.specifier);
    const generated = emitBindings(graph.rootType(module), id);
    const inputs = [...graph.inputs];
    modules.push({ id, resolved, specifier: entry.specifier, file: entry.file, ...generated, inputs });
  }
  let bundle;
  if (request.runtime) {
    bundle = await bundleModules({ ...request, modules }, directory);
    for (const module of modules) module.inputs.push(...bundle.inputs);
  }
  return { modules, bundle };
}

export async function bundleModules(request, directory) {
    const entry = request.modules.map((module, index) => `import * as m${index} from ${JSON.stringify(module.resolved)};`).join("\n")
      + `\nexport const importedJsModules = {${request.modules.map((module, index) => `${JSON.stringify(`dream.js.${module.id}`)}: { namespace: () => m${index} }`).join(",")}};\n`;
    const result = await esbuild.build({
      stdin: { contents: entry, resolveDir: directory, sourcefile: "dream-js-imports.mjs", loader: "js" },
      bundle: true, write: false, format: "esm", platform: request.target === "web" ? "browser" : "node",
      target: request.target === "web" ? "es2022" : "node18", metafile: true,
      logLevel: "silent", legalComments: "eof", absWorkingDir: directory,
    });
    return { code: result.outputFiles[0].text, inputs: Object.keys(result.metafile.inputs).filter(file => !file.startsWith("<")).map(file => path.resolve(directory, file)) };
  }

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    let input = "";
    for await (const chunk of process.stdin) input += chunk;
    process.stdout.write(JSON.stringify(await processImports(JSON.parse(input))));
  } catch (error) {
    process.stderr.write(`${error.message}\n`);
    process.exitCode = 1;
  }
}
