import path from "node:path";
import doctrine from "doctrine";

const atom = kind => ({ kind, name: kind, key: kind });
export const primitive = Object.fromEntries(["string", "number", "boolean", "bigint", "void", "null", "undefined", "any"].map(kind => [kind, atom(kind)]));
export const propertyName = node => node?.name ?? node?.value ?? "";
const unwrap = node => node?.type === "TSTypeAnnotation" ? node.typeAnnotation : node;
const callable = signatures => ({ kind: "function", name: "callback", key: "callback", signatures });

export class BindingTypes {
  constructor(graph) { this.graph = graph; this.cache = new Map(); }
  leadingComment(symbol) {
    const node = symbol.nodes?.[0];
    if (!node) return null;
    const comments = symbol.module.comments;
    let low = 0, high = comments.length;
    while (low < high) {
      const mid = (low + high) >>> 1;
      if (comments[mid].end <= node.start) low = mid + 1; else high = mid;
    }
    const comment = comments[low - 1];
    if (!comment || !comment.value.startsWith("*")) return null;
    const gap = symbol.module.text.slice(comment.end, node.start);
    if (gap.length > 128 || gap.replace(/\bexport\b|\bdeclare\b|\bconst\b|\blet\b|\bvar\b/g, "").trim()) return null;
    return doctrine.parse(comment.value, {unwrap:true,sloppy:true,recoverable:true});
  }
  location(file) { return (file.includes("node_modules/") ? file.split("node_modules/").at(-1) : path.relative(this.graph.root, file)).replaceAll("\\", "/"); }
  documentation(symbol) { return this.leadingComment(symbol)?.description ?? ""; }
  docTypes(symbol) { return this.leadingComment(symbol)?.tags ?? []; }
  jsdoc(type) {
    if (!type) return primitive.any;
    if (type.type === "NameExpression") return primitive[type.name] ?? primitive.any;
    if (type.type === "VoidLiteral") return primitive.void;
    if (type.type === "NullLiteral") return primitive.null;
    if (type.type === "UnionType") return this.union(type.elements.map(type => this.jsdoc(type)));
    if (type.type === "TypeApplication" && ["Promise", "Array"].includes(type.expression.name)) {
      const value = this.jsdoc(type.applications[0]);
      return { kind: type.expression.name === "Promise" ? "promise" : "array", value, name: `${type.expression.name}<${value.name}>`, key: `${type.expression.name}<${value.key}>` };
    }
    if (type.type === "RecordType") return { kind: "object", name: "Object", key: JSON.stringify(type), properties: () => type.fields.map(field => ({ name: field.key, type: () => this.jsdoc(field.value) })) };
    return primitive.any;
  }
  union(types) { return { kind: "union", types, key: types.map(type => type.key).join("|"), name: types.map(type => type.name).join(" | ") }; }
  node(node, module, bindings = new Map()) {
    node = unwrap(node);
    if (!node) return primitive.any;
    const keywords = { TSStringKeyword: "string", TSNumberKeyword: "number", TSBooleanKeyword: "boolean", TSBigIntKeyword: "bigint", TSVoidKeyword: "void", TSUndefinedKeyword: "undefined", TSNullKeyword: "null", TSAnyKeyword: "any", TSUnknownKeyword: "any" };
    if (keywords[node.type]) return primitive[keywords[node.type]];
    if (node.type === "TSLiteralType") return node.literal.value === null ? primitive.null : primitive[typeof node.literal.value] ?? primitive.any;
    if (node.type === "TSUnionType") return this.union(node.types.map(type => this.node(type, module, bindings)));
    if (node.type === "TSArrayType") {
      const value = this.node(node.elementType, module, bindings);
      return { kind: "array", value, name: `${value.name}[]`, key: `${value.key}[]` };
    }
    if (node.type === "TSParenthesizedType") return this.node(node.typeAnnotation, module, bindings);
    if (node.type === "TSIntersectionType") {
      const types = node.types.map(type => this.node(type, module, bindings));
      return { kind: "object", name: types.map(type => type.name).join("_"), key: types.map(type => type.key).join("&"), namespace:module.namespace, properties: () => [...new Map(types.flatMap(type => type.properties?.() ?? []).map(property => [property.name, property])).values()] };
    }
    if (node.type === "TSFunctionType") return callable([this.signature(node, module, bindings)]);
    if (node.type === "TSTypeLiteral") return this.object(`literal:${this.location(module.file)}:${node.start}`, "Object", [node], module, bindings);
    if (node.type === "TSTypeReference") {
      const name = propertyName(node.typeName);
      const args = (node.typeArguments?.params ?? []).map(arg => this.node(arg, module, bindings));
      if (bindings.has(name)) return bindings.get(name);
      if (["Promise", "PromiseLike", "Array", "ReadonlyArray"].includes(name)) return { kind: name.startsWith("Promise") ? "promise" : "array", value: args[0] ?? primitive.any, name: `${name}<${args.map(arg => arg.name).join(",")}>`, key: `${name}<${args.map(arg => arg.key).join(",")}>` };
      let symbol;
      if (node.typeName.type === "TSQualifiedName") {
        const imported = module.imports.get(propertyName(node.typeName.left));
        if (imported) symbol = this.graph.exported(imported.module, propertyName(node.typeName.right));
      } else symbol = this.graph.symbol(module, name);
      return symbol ? this.reference(symbol, args) : { ...primitive.any, name: name || "unresolved type" };
    }
    if (node.type === "TSTypeQuery") {
      const symbol = this.graph.symbol(module, propertyName(node.exprName));
      return symbol ? this.value(symbol) : primitive.any;
    }
    if (node.type === "TSThisType") return bindings.get("this") ?? primitive.any;
    return { ...primitive.any, name: module.text.slice(node.start, node.end).slice(0, 200) || node.type };
  }
  reference(symbol, args = []) {
    const relative = symbol.module.file.includes("node_modules/") ? symbol.module.file.split("node_modules/").at(-1) : path.relative(this.graph.root, symbol.module.file);
    const key = `${relative}#${symbol.name}<${args.map(arg => arg.key).join(",")}>`;
    if (this.cache.has(key)) return this.cache.get(key);
    const node = symbol.nodes[0];
    const bindings = new Map((node.typeParameters?.params ?? []).map((param, index) => [propertyName(param.name), args[index] ?? this.node(param.default, symbol.module)]));
    if (node.type === "TSTypeAliasDeclaration") {
      this.cache.set(key, primitive.any);
      const type = this.node(node.typeAnnotation, symbol.module, bindings);
      this.cache.set(key, type); return type;
    }
    if (node.type === "TSEnumDeclaration") return primitive.number;
    const type = this.object(key, symbol.name, symbol.nodes, symbol.module, bindings);
    this.cache.set(key, type);
    bindings.set("this", type);
    return type;
  }
  parentTypes(nodes, module, bindings) {
    const parents = [];
    for (const node of nodes) {
      const list = node.superClass ? [{ expression: node.superClass, typeArguments: node.superTypeArguments }] : node.extends ?? [];
      for (const parent of list) {
        const type = this.node({ type: "TSTypeReference", typeName: parent.expression, typeArguments: parent.typeArguments }, module, bindings);
        if (type?.kind === "object") parents.push(type);
      }
    }
    return parents;
  }
  object(key, name, nodes, module, bindings) {
    let properties, parents;
    return { kind: "object", key, name, namespace: module.namespace,
      constructors: nodes.flatMap(node => node.members ?? node.body?.body ?? []).filter(member => member.type === "TSConstructSignatureDeclaration").map(member => this.signature(member,module,bindings)),
      signatures: nodes.flatMap(node => node.members ?? node.body?.body ?? []).filter(member => member.type === "TSCallSignatureDeclaration").map(member => this.signature(member,module,bindings)),
      parents: () => parents ??= this.parentTypes(nodes, module, bindings),
      properties: () => properties ??= this.members(nodes, module, bindings, false),
      index: () => {
        const member = nodes.flatMap(node => node.members ?? node.body?.body ?? []).find(member => member.type === "TSIndexSignature");
        return member ? this.node(member.typeAnnotation, module, bindings) : null;
      },
    };
  }
  members(nodes, module, bindings, staticOnly) {
    const properties = new Map();
    for (const node of nodes) {
      for (const member of node.members ?? node.body?.body ?? []) {
        if (!!member.static !== staticOnly || ["private", "protected"].includes(member.accessibility) || member.computed) continue;
        const name = propertyName(member.key);
        if (!name || name === "constructor") continue;
        const method = member.type === "TSMethodSignature" || member.type === "MethodDefinition";
        const signatureNode = member.value ?? member;
        if (method && member.kind !== "get" && member.kind !== "set") {
          const signature = this.signature(signatureNode, module, bindings);
          const previous = properties.get(name);
          if (previous?.method) previous.signatures.push(signature);
          else properties.set(name, { name, method: true, signatures: [signature], type: () => callable(properties.get(name).signatures), doc: this.documentation({nodes:[member],module}) });
        } else if (member.kind !== "set") {
          properties.set(name, { name, readonly: member.readonly || member.kind === "get", doc: this.documentation({nodes:[member],module}), type: () => {
            const type = this.node(member.kind === "get" ? signatureNode.returnType : member.typeAnnotation, module, bindings);
            return member.optional ? this.union([type, primitive.undefined]) : type;
          } });
        }
      }
    }
    return [...properties.values()];
  }
  signature(node, module, bindings = new Map(), tags = []) {
    return {
      params: node.params.map((param, index) => {
        const rest = param.type === "RestElement";
        const argument = rest ? param.argument : param.type === "AssignmentPattern" ? param.left : param;
        const tag = tags.find(tag => tag.title === "param" && tag.name === argument.name);
        return { name: argument.name ?? `arg${index}`, optional: argument.optional || param.type === "AssignmentPattern", rest,
          type: this.node(param.typeAnnotation ?? argument.typeAnnotation, module, bindings).kind !== "any" ? this.node(param.typeAnnotation ?? argument.typeAnnotation, module, bindings) : this.jsdoc(tag?.type) };
      }),
      result: node.returnType ? this.node(node.returnType, module, bindings) : this.jsdoc(tags.find(tag => ["return", "returns"].includes(tag.title))?.type),
    };
  }
  value(symbol) {
    if (symbol.namespaceModule) return this.graph.rootType(symbol.namespaceModule);
    const node = symbol.nodes.find(node => !["TSInterfaceDeclaration", "TSTypeAliasDeclaration"].includes(node.type)) ?? symbol.nodes[0];
    if (["FunctionDeclaration", "TSDeclareFunction"].includes(node.type)) return callable(symbol.nodes.map(node => this.signature(node, symbol.module, new Map(), this.docTypes(symbol))));
    if (node.type === "ClassDeclaration") {
      const instance = this.reference(symbol);
      const signatures = node.body.body.filter(member => member.kind === "constructor").map(member => ({ ...this.signature(member.value, symbol.module), result: instance }));
      return { kind: "object", name: `${symbol.name}Constructor`, key: `${instance.key}:constructor`, constructors: signatures.length ? signatures : [{params:[],result:instance}], properties: () => this.members(symbol.nodes, symbol.module, new Map(), true) };
    }
    if (node.type === "TSEnumDeclaration") return { kind: "object", name: symbol.name, key: `${this.location(symbol.module.file)}#${symbol.name}`, properties: () => node.body.members.map(member => ({ name: propertyName(member.id), readonly:true, type: () => typeof member.initializer?.value === "string" ? primitive.string : primitive.number })) };
    if (node.type === "VariableDeclarator") {
      if (node.id.typeAnnotation) return this.node(node.id.typeAnnotation, symbol.module);
      if (node.init?.type === "Literal") return primitive[typeof node.init.value] ?? primitive.any;
      if (["ArrowFunctionExpression", "FunctionExpression"].includes(node.init?.type)) return callable([this.signature(node.init, symbol.module, new Map(), this.docTypes(symbol))]);
    }
    return primitive.any;
  }
}
