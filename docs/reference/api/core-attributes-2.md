# Attributes

**Import:** `import system.core;`

## `ExportAttribute`

Exports a non-generic synchronous function through a plain C ABI wrapper.

```dream
public struct ExportAttribute
```

## `IntrinsicAttribute`

Binds an external declaration to the specified built-in operation.

```dream
public struct IntrinsicAttribute
```

## `operation`

Operation associated with this intrinsic declaration.

```dream
public operation: Intrinsic
```

## `InlineAttribute`

Requests inlining of the annotated function or method; it is a hint rather than a guarantee.

```dream
public struct InlineAttribute
```

## `NoinlineAttribute`

Prevents inlining of the annotated function or method.

```dream
public struct NoinlineAttribute
```

## `UnsafeAttribute`

Marks a function/method as unsafe. Calls are only allowed from other `@unsafe` contexts.

```dream
public struct UnsafeAttribute
```

## `AllowCycleAttribute`

Acknowledges that this class can form strong ownership cycles. ARC does not collect cycles; use weak/unowned back-links or explicit teardown to avoid leaks.

```dream
public struct AllowCycleAttribute
```

## `ConsumingAttribute`

Marks an extern whose reference parameters transfer ownership to the host, which must release them. Extern parameters are otherwise passed borrowed, since a host cannot release a Dream reference.

```dream
public struct ConsumingAttribute
```

## `OwnedAttribute`

The extern's result transfers ownership to Dream. On a `@cpp` member returning `T*`, the Dream object deletes it. On a `@c` extern returning `OwnedCPtr`, `@owned("free_fn")` names the C function that frees the pointer when the last reference is released.

```dream
public struct OwnedAttribute
```

## `free_function`

Optional function used to release foreign storage.

```dream
public free_function: Option<string>
```

## `JsAttribute`

Binds an external function to a JavaScript export using `@js("module", "export")`. Available only on JavaScript platforms.

```dream
public struct JsAttribute
```

## `module_name`

Fully qualified module containing the declaration.

```dream
public module_name: string
```

## `export_name`

Name exposed to callers of the foreign function.

```dream
public export_name: string
```

## `RuntimeAttribute`

Binds an external function to a named Dream host service on supported platforms.

```dream
public struct RuntimeAttribute
```

## `export_name`

Name exposed to callers of the foreign function.

```dream
public export_name: string
```

## `AsyncHostAttribute`

Deferred native host for an `extern async fun`: the `<host>Async` C symbol takes the future as its leading argument, performs the work off-thread, and completes it via the bound dream_complete_foreign. The run loop stays parked while the work is in flight.

```dream
public struct AsyncHostAttribute
```

## `NativeAttribute`

Marks a function/method as available on the native (C) host. Combine with `@node`/`@web` to restrict; absent all three means every runtime.

```dream
public struct NativeAttribute
```

## `NodeAttribute`

Marks a function/method as available on the Node.js host. Combine with `@native`/`@web` to restrict; absent all three means every runtime.

```dream
public struct NodeAttribute
```

## `WebAttribute`

Marks a function/method as available in the browser host. Combine with `@native`/`@node` to restrict; absent all three means every runtime.

```dream
public struct WebAttribute
```
