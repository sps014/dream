# GenToolResult

**Import:** `import system.codegen;`

## `GenToolResult`

Result of a tool requested during generation, including output and files.

```dream
public class GenToolResult
```

## `id`

Identifier of the tool run that produced this result.

```dream
public id: string
```

## `exit`

Process exit code.

```dream
public exit: int
```

## `stdout`

Text captured from standard output.

```dream
public stdout: string
```

## `stderr`

Text captured from standard error.

```dream
public stderr: string
```

## `files`

Files available to this operation.

```dream
public files: List<GenToolFile>
```

## `constructor`

Reads a tool result from its JSON representation.

```dream
public constructor(borrow v: JsonValue)
```
