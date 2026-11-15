# Requires Member Attribute

**Import:** `import system.core;`

## `RequiresMemberAttribute`

Requires the type bound to the generic parameter `parameter` to provide `member` at every call of the function. A call whose type lacks it reports `message`, with `{type}` replaced by the bound type. A method on a generic extension uses a requirement without a message to state what its own type parameters need, and the caller's requirement checks those transitively.

```dream
public struct RequiresMemberAttribute
```

## `parameter`

Generic parameter whose bound type must provide the member.

```dream
public parameter: string
```

## `member`

Method name the bound type must declare, directly or through an extension.

```dream
public member: string
```

## `message`

Error reported at the call when the requirement fails.

```dream
public message: Option<string>
```
