# Process Info

**Import:** `import system.process;`

Read the [process guide](../../library/system/process.md) for launch settings, cancellation, and streaming ownership.

## `ProcessInfo`

One row from `Process.list`: an operating-system process id, its parent, and its name.

```dream
public class ProcessInfo
```

## `pid`

Operating-system process id.

```dream
public get pid(): int
```

## `parent_pid`

Parent process id, or `0` when the host does not report one.

```dream
public get parent_pid(): int
```

## `name`

Process name. Empty when the operating system does not reveal it.

```dream
public get name(): string
```
