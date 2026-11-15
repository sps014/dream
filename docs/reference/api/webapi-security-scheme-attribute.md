# SecuritySchemeAttribute

**Import:** `import system.webapi;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `SecuritySchemeAttribute`

Declares a dependency's OpenAPI authentication scheme.

```dream
public struct SecuritySchemeAttribute
```

## `id`

Identifier used to reference this security scheme in OpenAPI.

```dream
public id: string
```

## `kind`

OpenAPI scheme kind, such as `http`, `apiKey`, or `oauth2`.

```dream
public kind: string
```

## `scheme`

HTTP authentication scheme, such as `bearer`.

```dream
public scheme: string
```

## `name`

Name of the API-key header, query parameter, or cookie, when applicable.

```dream
public name: Option<string>
```

## `location`

Where an API key is supplied: `header`, `query`, or `cookie`.

```dream
public location: Option<string>
```
