import {readFileSync} from "node:fs";
import { digest } from "./resolve.mjs";

const reserved = new Set([...JSON.parse(readFileSync(new URL("./keywords.json", import.meta.url), "utf8")), "constructor", "del", "this"]);
const identifier = name => /^[A-Za-z_][A-Za-z0-9_]*$/.test(name) && (!reserved.has(name) || name === "default");
const quote = JSON.stringify;
const indent = text => text.split("\n").map(line => `    ${line}`).join("\n");
const documentation = text => {
  if (!text) return "";
  const flat = text.replaceAll("\r", "").replaceAll("\n", " ").replace(/\s+/g, " ").trim();
  const sentence = flat.match(/^.*?[.!?](?:\s|$)/)?.[0] ?? flat.slice(0, 240);
  return `/// ${sentence.trim()}\n`;
};

const anyType = { kind: "any", name: "any", key: "any" };

function objectKey(type) {
  if (type?.kind === "object") return type.key;
  if (type?.kind !== "union") return null;
  const present = type.types.filter(part => !["null", "undefined"].includes(part.kind));
  return present.length === 1 ? objectKey(present[0]) : null;
}

// Overloads that name different objects for one callback slot do not select an event. That slot is
// `js`. A slot that is the same object on every overload keeps that type.
function reconcileCallbacks(signatures) {
  const copies = signatures.map(signature => ({ ...signature, params: signature.params.map(param => ({ ...param })) }));
  const callback = type => (type?.kind === "function" || type?.signatures?.length) ? type.signatures?.[0] : null;
  const width = Math.max(0, ...copies.map(signature => signature.params.length));
  for (let index = 0; index < width; index++) {
    const shapes = copies.map(signature => callback(signature.params[index]?.type)).filter(Boolean);
    if (shapes.length < 2) continue;
    const disputed = new Set();
    const count = Math.max(...shapes.map(shape => shape.params.length));
    const keysAt = (choose) => shapes.map(choose).filter(Boolean);
    for (let inner = 0; inner < count; inner++) {
      if (new Set(keysAt(shape => objectKey(shape.params[inner]?.type))).size > 1) disputed.add(inner);
    }
    const disputeResult = new Set(keysAt(shape => objectKey(shape.result))).size > 1;
    if (!disputed.size && !disputeResult) continue;
    for (const signature of copies) {
      const shape = callback(signature.params[index]?.type);
      if (!shape) continue;
      signature.params[index] = {
        ...signature.params[index],
        type: {
          kind: "function",
          signatures: [{
            params: shape.params.map((param, inner) => ({ ...param, type: disputed.has(inner) ? anyType : param.type })),
            result: disputeResult ? anyType : shape.result,
          }],
        },
      };
    }
  }
  return copies;
}

export function emitBindings(rootType, id, global = false) {
  const types = new Map(), converters = new Map(), warnings = new Set();
  const namespace = `dream.js.generated.m${id}`;
  const rootName = `JsModule${id}`;
  const dynamic = (type, why) => {
    warnings.add(`${type.name}: ${why}; exposed as dynamic js`);
    return { name: "js", read: value => value, write: value => value };
  };
  const primitive = name => ({ name, read: value => ({
    string: `js.as_string(${value})`, double: `js.as_double(${value})`,
    bool: `js.as_bool(${value})`, long: `js.as_long(${value})`,
  })[name] ?? value, write: value => value });
  function map(type, input = false) {
    const primitives = { string:"string", number:"double", boolean:"bool", bigint:"long", void:"void" };
    if (primitives[type.kind]) return primitive(primitives[type.kind]);
    if (type.kind === "union") {
      const present = type.types.filter(type => !["null", "undefined"].includes(type.kind));
      if (present.length === 1 && present.length !== type.types.length) {
        const inner = map(present[0], input);
        if (input) return dynamic(type, "optional input values need a dynamic boundary");
        const helper = `__optional_${digest(inner.name)}`;
        converters.set(helper, `internal fun ${helper}(value: js): Option<${inner.name}> {\n    if value.is_null() {\n        return Option.None;\n    }\n    let present: ${inner.name} = ${inner.read("value")};\n    return Option.Some(present);\n}`);
        return { name: `Option<${inner.name}>`, read: value => `${helper}(${value})`, write: value => value };
      }
      if (type.types.length && type.types.every(candidate => candidate.kind === type.types[0].kind) && primitives[type.types[0].kind]) return primitive(primitives[type.types[0].kind]);
      return dynamic(type, "heterogeneous union");
    }
    if ((type.kind === "function" || type.signatures?.length) && input) {
      const signature = type.signatures?.[0];
      if (!signature) return dynamic(type, "callback has no signature");
      const params = signature.params.map(param => map(param.type, true));
      const result = map(signature.result);
      // One imported object stays concrete. `js` is the callback slot when the signatures do not
      // agree on that object, and for every other type the callback ABI already accepts as `js`.
      const slot = mapped => {
        if (["string", "double", "bool", "long", "void"].includes(mapped.name)) return mapped.name;
        if (mapped.name === "js" || mapped.name.startsWith("Option<")) return "js";
        return mapped.name;
      };
      return { name: `fun(${params.map(slot).join(", ")}): ${slot(result)}`, read: value => value, write: value => value };
    }
    if (["object", "array"].includes(type.kind)) {
      if (input && /(?:Options|Config|Init|Parameters)$/.test(type.name)) return dynamic(type, "dictionary input accepts a JS object");
      if (!types.has(type.key)) {
        const base = type.name.replace(/[^A-Za-z0-9_]/g, "_").slice(0, 80);
        types.set(type.key, { name: `Js${base}_${digest(type.key)}`, type });
      }
      const entry = types.get(type.key);
      return { name: entry.name, read: value => `${entry.name}(${value})`, write: value => `${value}.__handle` };
    }
    return dynamic(type, "type cannot be represented statically");
  }
  const helpers = new Map();
  const helperSources = new Map();
  function remember(owner, source) {
    const list = helperSources.get(owner.key) ?? [];
    list.push(source);
    helperSources.set(owner.key, list);
  }
  function declare(owner, property) {
    const key = `${owner.key}\n${property.name}`;
    if (helpers.has(key)) return helpers.get(key);
    const {name, doc} = property;
    const value = property.type();
    const operations = [];
    const add = (operation, source) => {
      const helper = `__member_${digest(`${key}\n${operations.length}`)}`;
      operations.push({ ...operation, helper });
      remember(owner, source(helper));
    };
    if (value.constructors?.length) {
      for (const operation of methods(name, value.constructors, doc, true)) add(operation, operation.source);
    } else if (value.signatures?.length) {
      for (const operation of methods(name, value.signatures, doc, false)) add(operation, operation.source);
    } else {
      const mapped = map(value);
      add({ render: receiver => `${documentation(doc)}public get ${name}(): ${mapped.name} {\n    return __HELPER(${receiver});\n}` }, helper => `internal fun ${helper}(__handle: js): ${mapped.name} {\n    let value = __handle[${quote(name)}];\n    return ${mapped.read("value")};\n}`);
      if (!property.readonly && mapped.name !== "void") {
        const present = value.kind === "union" ? value.types.filter(type => !["null", "undefined"].includes(type.kind)) : [];
        const nullable = present.length === 1 && present.length !== value.types?.length;
        const setter = nullable ? map(present[0]) : mapped;
        const write = (helper, type) => `internal fun ${helper}(__handle: js, value: ${type.name}): void {\n    __handle[${quote(name)}] = ${type.write("value")};\n}`;
        add({ render: receiver => `public set ${name}(value: ${setter.name}) {\n    __HELPER(${receiver}, value);\n}` }, helper => write(helper, setter));
        if (nullable) add({ render: receiver => `public set ${name}(value: ${mapped.name}) {\n    __HELPER(${receiver}, value);\n}` }, helper => `internal fun ${helper}(__handle: js, value: ${mapped.name}): void {\n    switch (value) {\n        Some(present) => {\n            __handle[${quote(name)}] = ${setter.write("present")};\n        }\n        None => {\n            __handle[${quote(name)}] = js.null;\n        }\n    }\n}`);
      }
    }
    helpers.set(key, operations);
    return operations;
  }
  function renderMembers(type, receiver) {
    const lines = [];
    const own = new Set((type.properties?.() ?? []).map(property => property.name));
    for (const property of type.properties?.() ?? []) {
      if (!identifier(property.name)) { warnings.add(`property '${property.name}' cannot be spelled as a Dream member`); continue; }
      for (const operation of declare(type, property)) lines.push(operation.render(receiver).replaceAll("__HELPER", operation.helper));
    }
    const seen = new Set(own);
    const visit = current => {
      for (const parent of current.parents?.() ?? []) {
        map(parent);
        for (const property of parent.properties?.() ?? []) {
          if (seen.has(property.name) || !identifier(property.name)) continue;
          seen.add(property.name);
          for (const operation of declare(parent, property)) lines.push(operation.render(receiver).replaceAll("__HELPER", operation.helper));
        }
        visit(parent);
      }
    };
    visit(type);
    return lines;
  }
  function methods(name, signatures, doc, construct = false) {
    const output = [], seen = new Set();
    for (const signature of reconcileCallbacks(signatures)) {
      const params = signature.params, rest = params.at(-1)?.rest;
      const fixed = rest ? params.slice(0, -1) : params;
      const optional = fixed.findIndex(param => param.optional);
      const minimum = optional < 0 ? fixed.length : optional;
      for (let count = minimum; count <= fixed.length; count++) {
        const args = fixed.slice(0, count).map((param, index) => ({ name: (identifier(param.name) && !reserved.has(param.name) && !param.name.startsWith("__")) ? param.name : `arg${index}`, type: map(param.type, true) }));
        const declarations = args.map(arg => `${arg.name}: ${arg.type.name}`);
        const tailType = rest ? map(params.at(-1).type.kind === "array" ? params.at(-1).type.value : params.at(-1).type, true) : null;
        if (rest && count === fixed.length) declarations.push(`...__args: ${tailType.name}[]`);
        const key = declarations.map(arg => arg.slice(arg.indexOf(":") + 1)).join(",");
        if (seen.has(key)) continue;
        seen.add(key);
        const async = signature.result.kind === "promise";
        const ret = map(async ? signature.result.value : signature.result);
        const resultType = async ? `Result<${ret.name === "void" ? "js" : ret.name}, JsError>` : ret.name;
        const values = args.map(arg => arg.type.write(arg.name));
        const hasRest = rest && count === fixed.length;
        const restArg = hasRest ? "__rest" : "[]";
        const prefix = hasRest ? `let __rest: js[] = [js.undefined; __args.length];\nfor (let __index in 0..__args.length) {\n    __rest[__index] = ${tailType.write("__args[__index]")};\n}\n` : "";
        const call = construct ? `js.construct(___RECEIVER___[${quote(name)}], [${values.join(", ")}], ${restArg})` : `js.apply(___RECEIVER___[${quote(name)}], ___RECEIVER___, [${values.join(", ")}], ${restArg})`;
        let body;
        if (async) body = `let __promise = ${call};\nlet __settled = __promise.await;\nreturn switch (__settled) {\n    Ok(__result) => Result.Ok(${ret.name === "void" ? "__result" : ret.read("__result")}),\n    Err(error) => Result.Err(error),\n};`;
        else if (ret.name === "void") body = `${call};`;
        else body = `let __result = ${call};\nreturn ${ret.read("__result")};`;
        const argList = [...args.map(arg => arg.name), ...(hasRest ? ["__args"] : [])];
        output.push({
          declarations, resultType, async, argList, body: prefix + body,
          render: receiver => {
            const invocation = `__HELPER(${[receiver, ...argList].join(", ")})`;
            const statement = async ? `return ${invocation}.await;` : resultType === "void" ? `${invocation};` : `return ${invocation};`;
            return `${documentation(doc)}public ${async ? "async " : ""}fun ${name}(${declarations.join(", ")}): ${resultType} {\n    ${statement}\n}`;
          },
          source: helper => `internal ${async ? "async " : ""}fun ${helper}(__handle: js${declarations.length ? `, ${declarations.map(declaration => declaration.replace(/^\.\.\./, "")).join(", ")}` : ""}): ${resultType} {\n${indent((prefix + body).replaceAll("___RECEIVER___", "__handle"))}\n}`,
        });
      }
    }
    return output;
  }
  function members(type, receiver, isStatic, globals = false) {
    const out = [];
    for (const property of type.properties?.() ?? []) {
      const {name, doc} = property;
      if (!identifier(name)) { warnings.add(`property '${name}' cannot be spelled as a Dream member`); continue; }
      const value = property.type();
      if (value.constructors?.length) {
        const generated = methods(name, value.constructors, doc, true);
        out.push(...generated.map(operation => `${documentation(doc)}public ${isStatic ? "static " : ""}${operation.async ? "async " : ""}fun ${name}(${operation.declarations.join(", ")}): ${operation.resultType} {\n${indent(operation.body.replaceAll("___RECEIVER___", receiver))}\n}`));
        if (!globals && value.properties?.().length) {
          const mapped = map(value);
          out.push(`${documentation(doc)}public ${isStatic ? "static " : ""}get ${name}(): ${mapped.name} {\n    let value = ${receiver}[${quote(name)}];\n    return ${mapped.read("value")};\n}`);
        }
      }
      else if (value.signatures?.length) {
        const generated = methods(name, value.signatures, doc, false);
        out.push(...generated.map(operation => `${documentation(doc)}public ${isStatic ? "static " : ""}${operation.async ? "async " : ""}fun ${name}(${operation.declarations.join(", ")}): ${operation.resultType} {\n${indent(operation.body.replaceAll("___RECEIVER___", receiver))}\n}`));
      }
      else {
        const mapped = map(value);
        if (globals) {
          out.push(`${documentation(doc)}public const ${name}: ${mapped.name} = ${mapped.read(`${receiver}[${quote(name)}]`)};`);
        } else {
          out.push(`${documentation(doc)}public ${isStatic ? "static " : ""}get ${name}(): ${mapped.name} {\n    let value = ${receiver}[${quote(name)}];\n    return ${mapped.read("value")};\n}`);
          if (!property.readonly && !isStatic && mapped.name !== "void") {
            const present = value.kind === "union" ? value.types.filter(type => !["null", "undefined"].includes(type.kind)) : [];
            const nullable = present.length === 1 && present.length !== value.types?.length;
            const setter = nullable ? map(present[0]) : mapped;
            out.push(`public set ${name}(value: ${setter.name}) {\n    ${receiver}[${quote(name)}] = ${setter.write("value")};\n}`);
            if (nullable) out.push(`public set ${name}(value: ${mapped.name}) {\n    switch (value) {\n        Some(present) => {\n            ${receiver}[${quote(name)}] = ${setter.write("present")};\n        }\n        None => {\n            ${receiver}[${quote(name)}] = js.null;\n        }\n    }\n}`);
          }
        }
      }
    }
    return out;
  }
  const files = [], header = `module ${namespace};\n\nimport system.js;\n\n`;
  if (global) files.push({ name: "globals.dream", source: header + members(rootType, "js.global", false, true).join("\n\n") + "\n" });
  else {
    const load = `@node\n@web\n@js(${quote(`dream.js.${id}`)}, "namespace")\ninternal static extern fun __module(): js;`;
    files.push({ name: "bindings.dream", source: `${header}import dream.js.types;\n\n/// JavaScript exports resolved from the imported module.\n@js_type(${quote(rootType.namespace ?? "JavaScript")})\npublic static class ${rootName} {\n${indent([load, ...members(rootType, `${rootName}.__module()`, true)].join("\n\n"))}\n}\n` });
  }
  for (const entry of types.values()) {
    const body = ["internal __handle: js;", "internal constructor(value: js) {\n    this.__handle = value;\n}"];
    if (entry.type.kind === "array") body.push("public get length(): int {\n    let length: int = this.__handle.length;\n    return length;\n}");
    else body.push(...renderMembers(entry.type, "this.__handle"));
    const index = entry.type.kind === "array" ? entry.type.value : entry.type.index?.();
    if (index) {
      const mapped = map(index);
      body.push(`public fun this[index: int]: ${mapped.name} {\n    let value = this.__handle[index];\n    return ${mapped.read("value")};\n}`);
    }
    const shared = (helperSources.get(entry.type.key) ?? []).join("\n\n");
    files.push({ name: `${entry.name.toLowerCase()}.dream`, source: `${header.replace(`module ${namespace};`, "module dream.js.types;")}/// Live JavaScript reference to ${entry.type.name}.\n@js_type(${quote(entry.type.namespace ?? rootType.namespace ?? "JavaScript")})\npublic struct ${entry.name} {\n${indent(body.join("\n\n"))}\n}\n${shared ? `\n${shared}\n` : ""}` });
  }
  for (const [name, contents] of converters) files.push({ name: `${name}.dream`, source: "module dream.js.types;\n\nimport system.js;\n\n" + contents + "\n" });
  files.push({ name: "types.dream", source: "module dream.js.types;\n" });
  return { namespace, rootName, files, warnings: [...warnings].sort() };
}
