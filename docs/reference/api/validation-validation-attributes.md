# Validation Attributes

**Import:** `import system.validation;`

See [model validation](../../web-apis/validation.md) for strict decoding, constraints, defaults, and callbacks.

## `public struct MinimumAttribute`

Inclusive lower numeric bound.

```dream
public struct MinimumAttribute
```

## `public struct MaximumAttribute`

Inclusive upper numeric bound.

```dream
public struct MaximumAttribute
```

## `public struct MinLengthAttribute`

Minimum string or collection length.

```dream
public struct MinLengthAttribute
```

## `public struct MaxLengthAttribute`

Maximum string or collection length.

```dream
public struct MaxLengthAttribute
```

## `public struct PatternAttribute`

Regular expression required for a string.

```dream
public struct PatternAttribute
```

## `public struct DefaultJsonAttribute`

JSON encoded field default, applied only when the property is absent.

```dream
public struct DefaultJsonAttribute
```

## `public struct AllowExtraAttribute`

Permits additional input properties on a model; the default is to reject them.

```dream
public struct AllowExtraAttribute
```

## `public struct ValidateWithAttribute`

A synchronous field validator returning an optional error message.

```dream
public struct ValidateWithAttribute
```

## `public struct ModelValidatorAttribute`

A static model validator returning structured issues after fields pass validation.

```dream
public struct ModelValidatorAttribute
```
