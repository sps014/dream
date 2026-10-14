# Io Error

**Import:** `import system.io;`

## `public class IoError : Error`

Filesystem / path failure implementing `Error`.

```dream
public class IoError : Error
```

## `public path: string`

Path involved in the failure (may be empty for non-path errors).

```dream
public path: string
```

## `public constructor`

Creates an error with path, machine code, and message.

```dream
public constructor(path: string, code: string, message: string)
```

## `public fun message`

Human-readable description.

```dream
public fun message(): string
```

## `public fun code`

Stable machine code (`ENOENT`, `EACCES`, `EIO`, `EEXIST`, …).

```dream
public fun code(): string
```

## `public static fun not_found`

File or directory does not exist.

```dream
public static fun not_found(path: string): IoError
```

## `public static fun permission_denied`

Permission was denied for the operation.

```dream
public static fun permission_denied(path: string): IoError
```

## `public static fun other`

Generic I/O failure with a custom message.

```dream
public static fun other(path: string, message: string): IoError
```

## `public static fun exists`

Path already exists when exclusive create was requested.

```dream
public static fun exists(path: string): IoError
```

## `public static fun closed`

Creates an `ECLOSED` error for an operation on a closed resource.

```dream
public static fun closed(path: string): IoError
```

## `public static fun invalid`

Creates an `EINVAL` error for an invalid argument or operation.

```dream
public static fun invalid(path: string, message: string): IoError
```

## `public static fun cancelled`

Cooperative cancellation (`CancellationToken`).

```dream
public static fun cancelled(): IoError
```
