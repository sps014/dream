# Core language boundary

The compiler knows the core language. It does not know library packages.

User programs still import `system.json`, `system.gpu`, and the rest the same way. Those packages compile as ordinary Dream. The compiler must not branch on their names.

## Core packages

`StdPackage.core_language` is set in the registry. These packages are core:

| Package | Why the compiler knows it |
| --- | --- |
| `system.core`, `system.primitives` | `string`, `Option`, `Result`, `Unit`, iteration and async protocols, `Weak`, spans, arrays, `Range`, `panic` |
| `system.collections` | List, set, and map literals, and `[v; n]` |
| `system.task` | `Task` |
| `system.js`, `system.interop` | `js`, `extern`, `jsimport` |
| `system.codegen` | Generator protocol |
| `system.testing` | `@test` and `dream test` |
| `system.simd` | `Vector<T>` |
| `system` | `CPtr`, `OwnedCPtr`, `NativeCallback`, `System.print`, `System.args` |

Everything else is a library package. That includes `system.json`, `system.validation`, `system.io`, `system.process`, `system.net`, `system.crypto`, `system.gpu`, `system.drawing`, `system.window`, `system.audio`, `system.app`, and the text, time, and web packages.

`system.io` and `system.process` are library packages. Their host capability field lists may still live next to the core host tables until every field is declared with `@runtime`.

## Naming a core declaration

Qualified identities live in `crates/dream-abi/src/core_items.rs` (`system.core::Option`, `system.collections::List`, and so on). Compiler lookups go through those constants and `TypeCtx::resolve` / `resolved_type`, which resolve `module::Name` inside that module. A user type with the same short name cannot shadow the core type.

When the compiler synthesizes a call (a collection literal, `Unit()` in `main`), it pins that identity for the callee only. Element expressions still resolve in the user's scope.

To add a core item:

1. The declaration's package must already be `core_language`.
2. Add a qualified constant in `core_items.rs`.
3. Resolve it with `resolved_type` or `is_core_def`. Do not compare short names.

## Library host data

A library package does not get a match arm in the compiler. Host wiring is data on the declaration or the package record.

`@runtime(export_name, host?, traits...)` binds an extern to a host service:

- `host` names the native capability library that implements the service (`"net"`, `"gpu"`, …). `"core"`, or omitting it, means the core host or the runtime itself. `runtime_host_capability` reads it, and the driver links the capability libraries the program's imports name.
- `traits` are `RuntimeTrait` values. `Component` marks a service the standalone WASI component adapter implements; a component build rejects every other host import. `Quiet` marks a service that never touches Dream storage or ownership, so the optimizer may move retains and releases across the call. Listing traits requires spelling the host.
- A runtime C module is linked when the program references one of its symbols. Regex and audio FFT are ordinary `@runtime` declarations whose C symbols use the field names.

`StdPackage` records carry the rest:

- `guest_exports` lists extra symbols the guest module must export when one of the package's host imports is used (the GPU bundle version and index).
- `web_assets` and `tools` list files the package ships next to the program.

`platform_main` is a core identity (`core_items::PLATFORM_MAIN`), not an attribute. On targets where the operating system owns the main thread, the generated `main` calls the host loop it names.

## Generic library code

Library code uses ordinary language features where the compiler used to rewrite calls:

- `T.member` and generic overloads let a generic function call a static member of its type argument.
- `@requires_member(param, member, message?)` states that a type argument must provide a member. The check runs at the call site, recursively through templates, and an extension method whose requirement fails is not attached to that instantiation. The message is the diagnostic users see.
- Static calls on a generic union (`Opt<T>.parse(...)`) resolve as static calls, not variant construction, when the union declares a method of that name.

`Json.serialize`, `Json.deserialize`, `Json.from_value`, and `Validation.*` are plain stdlib functions built from these pieces. The `@json` derive generator emits per-type methods, and nested collections fall back to `Json.from_value<T>`.

## Field defaults

Field defaults are core language. A field the constructor does not assign on every path starts as an empty array, `T()`, or the union's first variant without fields when its zero value is not a value of its type. A field with no such default is a compile error. `hir_emit/field_defaults.rs` assigns explicit-constructor defaults in a prologue and implicit-constructor defaults on `HExprKind::New::field_inits`. Stdlib sources are exempt and keep zeroed fields.

## Audit

`compiler_does_not_name_library_packages` in `crates/dream-stdlib` reads the non-test Rust sources of the compiler crates. It fails when a string literal is a non-core package name, a `<std>/` path from a non-core package, or a qualified name under a non-core package. `core_items.rs` is the only compiler file allowed to name core identities. The registry and the rest of `dream-stdlib` are the package data, so the audit does not scan them.

## Stdlib trust

Some checks exempt stdlib code (private intrinsics, field defaults, inlining limits). They test `dream_stdlib::is_std_source`, the `<std>/` virtual path prefix. Only the prelude loader assigns that prefix: program files are canonicalized to absolute paths, and `parse_source_recursive` rejects a `<std>/` path, so a program source cannot claim stdlib privileges.

## Host tables that stay data

- The browser and Node host chunks are listed in `runtime/src/chunks.manifest`, shared with `scripts/bundle-runtime.mjs`; the driver picks chunks by the program's live imports.
- Host effect summaries come from the `Quiet` trait on each `@runtime` declaration.
- Worker and process-argument host fields in `runtime_hosts.rs` serve `Task` and `System.args`, which are core.
