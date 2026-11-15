# AudioError

**Import:** `import system.audio;`

Read the [audio guide](../../library/system/audio.md) for usage, limits, and current platform restrictions.

## `AudioError`

A stable audio failure code and a readable explanation.

```dream
public class AudioError: Error
```

## `constructor`

Creates a failure with a machine-readable code and message.

```dream
public constructor(code: string, message: string)
```

## `code`

Returns the failure code, such as EINVAL, EFORMAT, ECLOSED, or ENOTSUP.

```dream
public fun code(): string
```

## `message`

Returns a readable explanation of the failure.

```dream
public fun message(): string
```
