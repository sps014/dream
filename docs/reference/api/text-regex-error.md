# RegexError

**Import:** `import system.text;`

## `RegexError`

Failure to compile a regular expression.

```dream
public class RegexError: Error
```

## `pattern`

Pattern that could not be compiled.

```dream
public pattern: string
```

## `message`

A description of the compilation failure.

```dream
public fun message(): string
```

## `code`

Stable compilation-failure code.

```dream
public fun code(): string
```
