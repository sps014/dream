# GenToolInfo

**Import:** `import system.codegen;`

## `GenToolInfo`

Identity and configuration of an available generation tool.

```dream
public class GenToolInfo
```

## `name`

Tool name as declared with `@uses_tool`.

```dream
public name: string
```

## `version`

Version identifying this API or snapshot format.

```dream
public version: string
```

## `fingerprint`

Content fingerprint identifying this version of the tool.

```dream
public fingerprint: string
```

## `available`

Whether the tool was found on this machine. Requesting an unavailable tool with `GenContext.run_tool` fails with the reason it could not be found, so a generator can skip a step that only an optional tool performs.

```dream
public available: bool
```

## `constructor`

Reads an available tool description from its JSON representation.

```dream
public constructor(borrow v: JsonValue)
```
