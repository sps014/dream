import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { processImports } from "../main.mjs";

async function fixture(t, declarations, implementation = "export const version = 'test';") {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "dream-js-test-"));
  t.after(() => fs.rm(root, {recursive:true,force:true}));
  await fs.writeFile(path.join(root,"library.d.mts"),declarations);
  await fs.writeFile(path.join(root,"library.mjs"),implementation);
  const request = {root,npm:{},target:"node",imports:[{specifier:"./library.mjs",file:path.join(root,"main.dream")}]};
  return {root,request,result:await processImports(request)};
}

test("declarations produce typed overloads, live references, promises and namespaces", async t => {
  const {result} = await fixture(t, `
    export interface Item { readonly name:string; count:number; child:Item|null; }
    export function read():Item;
    export function read(name:string):Item;
    export function load():Promise<Item|null>;
    export function each(handler:(value:number)=>boolean):void;
    export function tokens(...values:string[]):void;
    export default function add(a:number,b:number):number;
  `);
  const text = result.modules[0].files.map(file=>file.source).join("\n");
  assert.match(text,/fun read\(\)/);
  assert.match(text,/fun read\(name: string\)/);
  assert.match(text,/async fun load\(\): Result<Option<JsItem_/);
  assert.match(text,/fun each\(handler: fun\(double\): bool\)/);
  assert.match(text,/\.\.\.__args: string\[\]/);
  assert.match(text,/@js_type\("\.\/library.mjs"\)/);
  assert.match(text,/get name\(\): string/);
  assert.doesNotMatch(text,/set name\(/);
  assert.match(text,/set count\(value: double\)/);
  assert.match(text,/fun default\(a: double, b: double\): double/);
});

test("re-exports and imported type names share their definition identity", async t => {
  const {root,request} = await fixture(t, `export const placeholder:number;`);
  await fs.writeFile(path.join(root,"library.d.mts"), `export { create, Item } from './implementation.mjs';`);
  await fs.writeFile(path.join(root,"implementation.d.mts"),`export interface Item { name:string; } export function create():Item;`);
  await fs.writeFile(path.join(root,"implementation.mjs"),`export function create(){return {name:'ok'}}`);
  const output = await processImports(request);
  const text = output.modules[0].files.map(file=>file.source).join("\n");
  assert.match(text,/fun create\(\): JsItem_/);
  assert.match(text,/get name\(\): string/);
});

test("generated runtime bundles implementations without evaluating them", async t => {
  const {request,root} = await fixture(t, `export function value():number;`, `globalThis.__dreamBindingExecuted = true; export function value(){ return 42; }`);
  const output = await processImports({...request,runtime:true});
  assert.equal(globalThis.__dreamBindingExecuted,undefined);
  assert.match(output.bundle.code,/return 42/);
  assert.ok(output.bundle.inputs.includes(path.join(root,"library.mjs")));
});

test("overloaded listeners use js when the mapping names more than one event", async t => {
  const {result} = await fixture(t, `
    export interface MouseEvent { readonly button: number; }
    export interface KeyboardEvent { readonly key: string; }
    export interface Item { readonly name: string; }
    export function listen(type: "click", handler: (event: MouseEvent) => void): void;
    export function listen(type: "keydown", handler: (event: KeyboardEvent) => void): void;
    export function on(handler: (event: Item) => void): void;
  `);
  const text = result.modules[0].files.map(file => file.source).join("\n");
  assert.match(text, /fun listen\(arg0: string, handler: fun\(js\): void\)/);
  assert.doesNotMatch(text, /fun listen\(arg0: string, handler: fun\(Js(?:Mouse|Keyboard)Event_/);
  assert.match(text, /fun on\(handler: fun\(JsItem_/);
});

test("unsupported type operators are reported rather than guessed", async t => {
  const {result} = await fixture(t, `export function read<T>(value:T):T extends string ? number : boolean;`);
  assert.match(result.modules[0].files[0].source,/fun read\(value: js\): js/);
  assert.ok(result.modules[0].warnings.some(warning=>warning.includes("extends")));
});
