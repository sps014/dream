# API catalog

Use the [standard-library guides](../stdlib/index.md) when learning a feature. Use this catalog when you need to look up an exact public declaration, including an overload, property, or enum choice.

## Reading a signature

`name: Type` gives a parameter or field's type. A value after `=` is the default when you leave an argument out. The type after a method's closing `)` is its result.

- `static` means you call a member on its type, such as `Math.sqrt(9.0)`.
- A regular method needs a value of that type, such as `names.push("Ada")`.
- `get` and `set` describe properties; use `value.length`, without parentheses.
- `constructor` describes the arguments used when creating a value.
- `async` means you wait for completion with `.await` inside an async function.
- `Option<T>` means a value may be absent. [Check it before using it](../stdlib/option-result.md).
- `Result<T, E>` means an operation may fail. Handle `Ok` and `Err`, or pass the failure to your caller with `?`.
- `borrow` means the call uses a value without taking ownership. [Ownership](../language/ownership.md) explains when to use it.
- `<T>` names a type parameter. A condition such as `T : Comparable<T>` limits the types accepted.

These declaration snippets show the interface; they are not complete programs. The linked guides contain examples. Availability and integration requirements are explained in those guides; a declaration alone does not establish platform support.

## Packages

- [system](package-system.md)
- [system.codegen](system-codegen.md)
- [system.collections](system-collections.md)
- [system.core](system-core.md)
- [system.crypto](system-crypto.md)
- [system.encoding](system-encoding.md)
- [system.io](system-io.md)
- [system.json](system-json.md)
- [system.logging](system-logging.md)
- [system.primitives](system-primitives.md)
- [system.process](system-process.md)
- [system.simd](system-simd.md)
- [system.task](system-task.md)
- [system.testing](system-testing.md)
- [system.text](system-text.md)
