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

A library package does not get a match arm in the compiler. Host wiring is data:

- `package_host_capability` records which native capability owns a package (`system.net` → `net`).
- `runtime_field_capability` reads `@runtime("field")` from those packages. A field declared under two capabilities is left unset.
- Link selection still uses the compiler field tables. The scan cannot replace them yet: `cryptoAuth` is declared in `system.webapi` while the crypto library implements it, and `timeNowNanos` is shared by the core clock, window, and audio. Pointing the linker at the scan would attach the wrong capability or add one that today's programs do not link.
- A runtime C module is linked when the program references one of its symbols (`host_fields` or an intrinsic key).
- Regex and audio FFT are ordinary `@runtime` declarations. Their C symbols use the same names as the field strings.

The WASI component allow-list (`component_host_supported`) is the adapter's implemented surface, not a catalog of library APIs. The JS chunk manifest remains the browser and Node host layout.

## Audit

`compiler_does_not_name_library_packages` in `crates/dream-stdlib` reads the non-test Rust sources of the compiler crates. It fails when a string literal is a non-core package name, a `<std>/` path from a non-core package, or a qualified name under a non-core package. `core_items.rs` is the only compiler file allowed to name core identities. The registry and the rest of `dream-stdlib` are the package data, so the audit does not scan them.

## Still in the compiler

`Json.serialize`, `Json.deserialize`, `Json.from_value`, and `Validation.*` are still expanded by the analyzer. A Dream body cannot replace that expansion yet: the language server skips generators, and the diagnostic (`'Map<string, object>' cannot be serialized to JSON: …`) has to be reported from the type before any `write_json` method exists. Moving that check into the derive generator would change the language-server message. GPU export glue, drawing asset paths, and the mobile app entry wrapper are still named by the driver for the same reason: the generic package-declared mechanism is not in place, and deleting the special case would change emitted output.
