# GenToolInfo

**Import:** `import system.codegen;`

## `GenToolInfo`

Identity and configuration of an available generation tool.

```dream
public class GenToolInfo
```

## `name`

Name identifying this entry.

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

## `constructor`

Reads an available tool description from its JSON representation.

```dream
public constructor(borrow v: JsonValue)
```
