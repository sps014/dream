import fs from "node:fs/promises";
import {realpathSync} from "node:fs";
import path from "node:path";
import { parseSync, visitorKeys } from "oxc-parser";
import { ResolverFactory } from "oxc-resolver";
import { BindingTypes } from "./types.mjs";

export class DeclarationGraph {
  constructor(root, target) {
    this.root = realpathSync(root);
    this.modules = new Map();
    this.inputs = new Set();
    this.resolver = new ResolverFactory({ conditionNames: ["types", target === "web" ? "browser" : "node", "import", "default"], extensions: [".js", ".mjs", ".cjs", ".ts", ".mts", ".cts"], mainFields: ["types", "typings", "module", "main"], builtinModules: true });
    this.types = new BindingTypes(this);
  }
  resolve(file, specifier) {
    const result = this.resolver.resolveDtsSync(file, specifier);
    if (result.path) {
      if (result.packageJsonPath) this.inputs.add(result.packageJsonPath);
      return result.path;
    }
    const runtime = this.resolver.sync(path.dirname(file), specifier);
    if (runtime.packageJsonPath) this.inputs.add(runtime.packageJsonPath);
    if (runtime.path) return runtime.path;
    throw new Error(`cannot resolve '${specifier}' from ${file}: ${result.error ?? runtime.error}`);
  }
  async load(file, namespace) {
    file = path.resolve(file);
    if (this.modules.has(file)) return this.modules.get(file);
    const text = await fs.readFile(file, "utf8");
    const parsed = parseSync(file, text, { astType: "ts" });
    if (parsed.errors.length) throw new Error(`${file}: ${parsed.errors[0].message}`);
    const module = { file, namespace, text, comments: parsed.comments, symbols: new Map(), exports: new Map(), imports: new Map(), stars: [], dependencies: new Map() };
    this.modules.set(file, module);
    this.inputs.add(file);
    for (let parent = path.dirname(file); parent !== path.dirname(parent); parent = path.dirname(parent)) {
      this.inputs.add(path.join(parent, "package.json"));
      if (parent === this.root) break;
    }
    for (const extension of [".d.ts", ".d.mts", ".d.cts"]) this.inputs.add(file.replace(/\.[^.]+$/, extension));
    const body = parsed.program.body;
    for (const statement of body) {
      const declaration = statement.declaration ?? statement;
      const nodes = declaration.type === "VariableDeclaration" ? declaration.declarations : [declaration];
      for (const node of nodes) {
        const name = node.id?.name;
        if (!name) continue;
        let symbol = module.symbols.get(name);
        if (symbol) symbol.nodes.push(node);
        else module.symbols.set(name, symbol = { name, nodes: [node], module });
        if (statement.type === "ExportNamedDeclaration") module.exports.set(name, symbol);
        if (statement.type === "ExportDefaultDeclaration") module.exports.set("default", symbol);
      }
      if (statement.type === "ExportDefaultDeclaration" && declaration.type === "Identifier") module.exports.set("default", { local: declaration.name });
    }
    const specifiers = new Set();
    function visit(node) {
      if (!node || typeof node !== "object") return;
      if (node.source?.type === "Literal" && typeof node.source.value === "string") specifiers.add(node.source.value);
      if (node.type === "TSImportType") {
        const value = node.argument?.value ?? node.argument?.literal?.value;
        if (value) specifiers.add(value);
      }
      for (const key of visitorKeys[node.type] ?? []) {
        const child = node[key];
        if (Array.isArray(child)) child.forEach(visit); else visit(child);
      }
    }
    body.forEach(visit);
    for (const specifier of specifiers) {
      const resolved = this.resolve(file, specifier);
      const dependency = await this.load(resolved, specifier.startsWith(".") ? namespace : specifier);
      module.dependencies.set(specifier, dependency);
    }
    for (const statement of body) {
      if (statement.type === "ImportDeclaration") {
        const dependency = module.dependencies.get(statement.source.value);
        for (const spec of statement.specifiers) module.imports.set(spec.local.name, { module: dependency, name: spec.imported?.name ?? (spec.type === "ImportDefaultSpecifier" ? "default" : "*") });
      } else if (statement.type === "ExportAllDeclaration") {
        const dependency = module.dependencies.get(statement.source.value);
        if (statement.exported) module.exports.set(statement.exported.name, { module: dependency, name: "*" });
        else module.stars.push(dependency);
      } else if (statement.type === "ExportNamedDeclaration") {
        for (const spec of statement.specifiers) module.exports.set(spec.exported.name ?? spec.exported.value, statement.source
          ? { module: module.dependencies.get(statement.source.value), name: spec.local.name ?? spec.local.value }
          : { local: spec.local.name ?? spec.local.value });
      }
    }
    return module;
  }
  symbol(module, name, seen = new Set()) {
    const key = `${module.file}#${name}`;
    if (seen.has(key)) return null;
    seen.add(key);
    if (module.symbols.has(name)) return module.symbols.get(name);
    const imported = module.imports.get(name);
    if (imported) return this.exported(imported.module, imported.name, seen);
    return this.exported(module, name, seen);
  }
  exported(module, name, seen = new Set()) {
    const value = module.exports.get(name);
    if (value?.nodes) return value;
    if (value?.local) return this.symbol(module, value.local, seen);
    if (value?.module) return value.name === "*" ? { namespaceModule: value.module, name } : this.exported(value.module, value.name, seen);
    for (const dependency of module.stars) {
      const key = `${dependency.file}#export:${name}`;
      if (seen.has(key)) continue;
      seen.add(key);
      const found = this.exported(dependency, name, seen);
      if (found) return found;
    }
    return null;
  }
  names(module, seen = new Set()) {
    if (seen.has(module.file)) return [];
    seen.add(module.file);
    return [...new Set([...module.exports.keys(), ...module.stars.flatMap(dependency => this.names(dependency, seen))])];
  }
  rootType(module, names = this.names(module)) {
    return { kind: "object", namespace: module.namespace, name: "Module", key: `${module.file}#namespace`, properties: () => names.flatMap(name => {
      const symbol = this.exported(module, name) ?? module.symbols.get(name);
      if (!symbol || symbol.nodes?.every(node => ["TSInterfaceDeclaration", "TSTypeAliasDeclaration"].includes(node.type))) return [];
      return [{ name, readonly: true, doc: this.types.documentation(symbol), type: () => this.types.value(symbol) }];
    }) };
  }
}
