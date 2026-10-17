# StmtKind

**Import:** `import system.codegen;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `StmtKind`

Statement categories exposed to source generators.

```dream
public enum StmtKind
```

## `Block`

Identifies block statements.

```dream
Block
```

## `Unknown`

Identifies unknown statements.

```dream
Unknown
```

## `Assignment`

Identifies assignment statements.

```dream
Assignment
```

## `IndexAssignment`

Identifies index assignment statements.

```dream
IndexAssignment
```

## `MemberAssignment`

Identifies member assignment statements.

```dream
MemberAssignment
```

## `Declaration`

Identifies declaration statements.

```dream
Declaration
```

## `TupleDeclaration`

Identifies tuple declaration statements.

```dream
TupleDeclaration
```

## `FunctionInvocation`

Identifies function invocation statements.

```dream
FunctionInvocation
```

## `MethodInvocation`

Identifies method invocation statements.

```dream
MethodInvocation
```

## `Return`

Identifies return statements.

```dream
Return
```

## `Yield`

Identifies yield statements.

```dream
Yield
```

## `AwaitForEach`

Identifies await for each statements.

```dream
AwaitForEach
```

## `If`

Identifies if statements.

```dream
If
```

## `While`

Identifies while statements.

```dream
While
```

## `DoWhile`

Identifies do while statements.

```dream
DoWhile
```

## `For`

Identifies for statements.

```dream
For
```

## `ForEach`

Identifies for each statements.

```dream
ForEach
```

## `Labeled`

Identifies labeled statements.

```dream
Labeled
```

## `Break`

Identifies break statements.

```dream
Break
```

## `Continue`

Identifies continue statements.

```dream
Continue
```

## `Expression`

Identifies expression statements.

```dream
Expression
```

## `Await`

Identifies await statements.

```dream
Await
```

## `Switch`

Identifies switch statements.

```dream
Switch
```
