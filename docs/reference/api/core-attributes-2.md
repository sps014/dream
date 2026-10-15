# system.core: Attribute declarations

No import is needed.

```dream
public struct IntrinsicAttribute
```

Marks an extern function/method as a compiler intrinsic. The string argument is the intrinsic key (e.g. `"print"`).

```dream
public operation: Intrinsic
```

```dream
public struct InlineAttribute
```

Raises the inliner's size budget for this function/method. A compiler hint, not a guarantee.

```dream
public struct NoinlineAttribute
```

Never inline this function/method (neither the MIR inliner nor LLVM). Keeps benchmark sinks opaque.

```dream
public struct UnsafeAttribute
```

Marks a function/method as unsafe. Calls are only allowed from other `@unsafe` contexts.

```dream
public struct AllowCycleAttribute
```

Acknowledges that this class can form strong ownership cycles. ARC does not collect cycles; use weak/unowned back-links or explicit teardown to avoid leaks.

```dream
public struct ConsumingAttribute
```

Marks an extern whose reference parameters transfer ownership to the host, which must release them. Extern parameters are otherwise passed borrowed, since a host cannot release a Dream reference.

```dream
public struct OwnedAttribute
```

The extern's result transfers ownership to Dream. On a `@cpp` member returning `T*`, the Dream object deletes it. On a `@c` extern returning `OwnedCPtr`, `@owned("free_fn")` names the C function that frees the pointer when the last reference is released.

```dream
public free_function: Option<string>
```

```dream
public struct JsAttribute
```

Binds an extern function to a JavaScript host import: `@js("module", "export")`. JS-only (user interop, `js` type, WASM `env`/libm). Dream runtime hosts that exist on native and WASM use `@runtime` instead.

```dream
public module_name: string
```

```dream
public export_name: string
```

```dream
public struct RuntimeAttribute
```

Binds an extern to the Dream runtime host on WASM and native: `@runtime("fileRead")` imports `Dream.fileRead` and calls the C symbol `fileRead`.

```dream
public export_name: string
```

```dream
public struct AsyncHostAttribute
```

Deferred native host for an `extern async fun`: the `<host>Async` C symbol takes the future as its leading argument, performs the work off-thread, and completes it via the bound dream_complete_foreign. The run loop stays parked while the work is in flight.

```dream
public struct NativeAttribute
```

Marks a function/method as available on the native (C) host. Combine with `@node`/`@web` to restrict; absent all three means every runtime.

```dream
public struct NodeAttribute
```

Marks a function/method as available on the Node.js host. Combine with `@native`/`@web` to restrict; absent all three means every runtime.

```dream
public struct WebAttribute
```

Marks a function/method as available in the browser host. Combine with `@native`/`@node` to restrict; absent all three means every runtime.
